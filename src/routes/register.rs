use axum::{extract::State, Json};
use chrono::Utc;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

use crate::db::tables;
use crate::error::{AppError, Result};
use crate::models::{User, UserRecord};
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
/// Returns 409 Conflict if the user ID already exists.
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

    let user_id = payload.user_id.clone();
    let db = state.db.clone();

    tokio::task::spawn_blocking(move || {
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(tables::USERS)?;

            // Check if user already exists
            if table.get(user_id.as_str())?.is_some() {
                tracing::info!("User already exists: {}", user_id);
                return Err(AppError::UserAlreadyExists);
            }

            // Insert new user
            let record = UserRecord {
                created_at: Utc::now().timestamp(),
            };
            let bytes = bincode::serialize(&record)?;
            table.insert(user_id.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;

        tracing::info!("New user registered: {}", user_id);
        Ok(())
    })
    .await??;

    Ok(Json(RegisterResponse { success: true }))
}
