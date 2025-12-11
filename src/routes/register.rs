use axum::{extract::State, Json};
use chrono::Utc;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

use crate::db::tables;
use crate::error::{AppError, Result};
use crate::models::{User, UserRecord};
use crate::security::apply_pepper;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    #[serde(rename = "userId")]
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub success: bool,
}

/// Register a new user
///
/// Creates a new user record with the provided user ID (SHA-256 hash of username).
/// The client-provided user ID is combined with a server-side pepper before storage
/// to protect against rainbow table attacks.
///
/// Returns 409 Conflict if the user ID already exists.
///
/// # Security
/// - User ID is peppered: `stored_id = SHA256(client_user_id + pepper)`
/// - Pepper is stored in environment variable, not database
/// - Database breach alone cannot identify users by username
///
/// # Anti-Griefing
/// - Stricter rate limiting applied at router level
/// - User ID must be valid SHA-256 hash format
pub async fn register_user(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>> {
    // Validate user ID format (must be 64-char hex string)
    if !User::validate_id(&payload.user_id) {
        tracing::warn!("Invalid user ID format: {}", payload.user_id);
        return Err(AppError::InvalidInput(
            "User ID must be a valid SHA-256 hash (64 hex characters)".to_string(),
        ));
    }

    // Apply server-side pepper to the client-provided user ID
    // This protects against rainbow table attacks if the database is breached
    let peppered_user_id = apply_pepper(&payload.user_id, &state.config.user_id_pepper);
    let db = state.db.clone();

    tokio::task::spawn_blocking(move || {
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(tables::USERS)?;

            // Check if user already exists (using peppered ID)
            if table.get(peppered_user_id.as_str())?.is_some() {
                tracing::info!("User already exists (peppered)");
                return Err(AppError::UserAlreadyExists);
            }

            // Insert new user with peppered ID
            let record = UserRecord {
                created_at: Utc::now().timestamp(),
            };
            let bytes = bincode::serialize(&record)?;
            table.insert(peppered_user_id.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;

        tracing::info!("New user registered (peppered)");
        Ok(())
    })
    .await??;

    Ok(Json(RegisterResponse { success: true }))
}
