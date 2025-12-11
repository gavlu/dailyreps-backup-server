use axum::{Json, extract::State};
use chrono::Utc;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

const BINCODE_CONFIG: bincode::config::Configuration = bincode::config::standard();

use crate::AppState;
use crate::constants::ERR_USER_ID_MUST_BE_SHA256;
use crate::db::tables;
use crate::error::{AppError, Result};
use crate::models::{User, UserRecord};

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
/// Creates a new user record with the provided user ID (SHA-256 hash).
/// Returns 409 Conflict if the user ID already exists.
pub async fn register_user(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>> {
    // Validate user ID format (must be 64-char hex string)
    if !User::validate_id(&payload.user_id) {
        tracing::warn!("Invalid user ID format: {}", payload.user_id);
        return Err(AppError::InvalidInput(
            ERR_USER_ID_MUST_BE_SHA256.to_string(),
        ));
    }

    let db = state.db.clone();
    let user_id = payload.user_id.clone();

    tokio::task::spawn_blocking(move || {
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(tables::USERS)?;

            // Check if user already exists
            if table.get(user_id.as_str())?.is_some() {
                tracing::info!("User already exists");
                return Err(AppError::UserAlreadyExists);
            }

            // Insert new user
            let record = UserRecord {
                created_at: Utc::now().timestamp(),
            };
            let bytes = bincode::serde::encode_to_vec(&record, BINCODE_CONFIG)?;
            table.insert(user_id.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;

        tracing::info!("New user registered");
        Ok(())
    })
    .await??;

    Ok(Json(RegisterResponse { success: true }))
}
