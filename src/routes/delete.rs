use axum::{extract::State, Json};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

const BINCODE_CONFIG: bincode::config::Configuration = bincode::config::standard();

use crate::constants::{ERR_INVALID_STORAGE_KEY, ERR_INVALID_USER_ID};
use crate::db::tables;
use crate::error::{AppError, Result};
use crate::models::{Backup, BackupRecord, User};
use crate::routes::validate_signed_request;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct DeleteUserRequest {
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "storageKey")]
    pub storage_key: String,
    pub signature: String,
    pub timestamp: i64,
}

#[derive(Debug, Serialize)]
pub struct DeleteUserResponse {
    pub success: bool,
    pub message: String,
}

/// Delete user and all associated data
///
/// This endpoint permanently deletes:
/// - User record
/// - All backup data
/// - Rate limit records
/// - User backups index
///
/// # Security
/// - Requires HMAC signature verification
/// - Requires timestamp validation
/// - Verifies storage key belongs to user (proves password knowledge)
pub async fn delete_user(
    State(state): State<AppState>,
    Json(payload): Json<DeleteUserRequest>,
) -> Result<Json<DeleteUserResponse>> {
    // 1. Validate formats
    if !User::validate_id(&payload.user_id) {
        return Err(AppError::InvalidInput(ERR_INVALID_USER_ID.to_string()));
    }

    if !Backup::validate_storage_key(&payload.storage_key) {
        return Err(AppError::InvalidInput(ERR_INVALID_STORAGE_KEY.to_string()));
    }

    // 2. Verify HMAC signature and timestamp
    validate_signed_request(
        &payload.storage_key,
        &payload.signature,
        payload.timestamp,
        &state.config.app_secret_key,
    )?;

    let db = state.db.clone();
    let user_id = payload.user_id.clone();
    let storage_key = payload.storage_key.clone();

    tokio::task::spawn_blocking(move || -> Result<()> {
        let write_txn = db.begin_write()?;
        {
            // 3. Verify user exists
            let mut users = write_txn.open_table(tables::USERS)?;
            if users.get(user_id.as_str())?.is_none() {
                tracing::warn!("Delete attempt for non-existent user");
                return Err(AppError::UserNotFound);
            }

            // 4. Verify the storage key belongs to this user
            let backups_table = write_txn.open_table(tables::BACKUPS)?;
            if let Some(backup_bytes) = backups_table.get(storage_key.as_str())? {
                let (backup, _): (BackupRecord, _) =
                    bincode::serde::decode_from_slice(backup_bytes.value(), BINCODE_CONFIG)?;
                if backup.user_id != user_id {
                    tracing::warn!("Delete attempt with mismatched storage key");
                    return Err(AppError::InvalidInput(
                        "Invalid credentials - storage key does not match user".to_string(),
                    ));
                }
            } else {
                tracing::warn!("Delete attempt with invalid storage key");
                return Err(AppError::InvalidInput(
                    "Invalid credentials - storage key does not match user".to_string(),
                ));
            }
            drop(backups_table);

            // 5. Get all backup keys for this user
            let mut user_backups = write_txn.open_table(tables::USER_BACKUPS)?;
            let backup_keys: Vec<String> = user_backups
                .get(user_id.as_str())?
                .and_then(|b| {
                    bincode::serde::decode_from_slice::<Vec<String>, _>(b.value(), BINCODE_CONFIG)
                        .ok()
                        .map(|(v, _)| v)
                })
                .unwrap_or_default();

            // 6. Delete all backups
            let mut backups = write_txn.open_table(tables::BACKUPS)?;
            for key in &backup_keys {
                backups.remove(key.as_str())?;
            }
            drop(backups);

            // 7. Delete rate limits
            let mut rate_limits = write_txn.open_table(tables::RATE_LIMITS)?;
            rate_limits.remove(user_id.as_str())?;
            drop(rate_limits);

            // 8. Delete user_backups index
            user_backups.remove(user_id.as_str())?;
            drop(user_backups);

            // 9. Delete user
            users.remove(user_id.as_str())?;
        }
        write_txn.commit()?;

        tracing::info!("User and all associated data deleted");

        Ok(())
    })
    .await??;

    Ok(Json(DeleteUserResponse {
        success: true,
        message: "User and all associated data permanently deleted".to_string(),
    }))
}
