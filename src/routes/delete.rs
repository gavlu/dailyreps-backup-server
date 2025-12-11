use axum::{extract::State, Json};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

use crate::constants::MAX_TIMESTAMP_AGE_SECS;
use crate::db::tables;
use crate::error::{AppError, Result};
use crate::models::{BackupRecord, User};
use crate::security::{validate_timestamp, verify_hmac};
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
/// - Validates user ID and storage key formats
/// - Verifies storage key belongs to user (proves password knowledge)
///
/// # Note
/// This action is irreversible. All encrypted backup data will be permanently deleted.
pub async fn delete_user(
    State(state): State<AppState>,
    Json(payload): Json<DeleteUserRequest>,
) -> Result<Json<DeleteUserResponse>> {
    // 1. Validate user ID and storage key formats
    if !User::validate_id(&payload.user_id) {
        return Err(AppError::InvalidInput(
            "Invalid user ID format".to_string(),
        ));
    }

    if payload.storage_key.len() != 64 || !payload.storage_key.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::InvalidInput(
            "Invalid storage key format".to_string(),
        ));
    }

    // 2. Verify HMAC signature
    // Use storageKey as the signed data to prove ownership
    if !verify_hmac(&payload.storage_key, &payload.signature, &state.config.app_secret_key) {
        tracing::warn!(
            "Invalid HMAC signature for user deletion: {}",
            payload.user_id
        );
        return Err(AppError::InvalidSignature);
    }

    // 3. Validate timestamp (prevent replay attacks)
    if !validate_timestamp(payload.timestamp, MAX_TIMESTAMP_AGE_SECS) {
        return Err(AppError::InvalidInput(
            "Timestamp too old or in the future".to_string(),
        ));
    }

    let db = state.db.clone();
    let user_id = payload.user_id.clone();
    let storage_key = payload.storage_key.clone();

    tokio::task::spawn_blocking(move || -> Result<()> {
        let write_txn = db.begin_write()?;
        {
            // 4. Verify user exists
            let mut users = write_txn.open_table(tables::USERS)?;
            if users.get(user_id.as_str())?.is_none() {
                tracing::warn!("Delete attempt for non-existent user: {}", user_id);
                return Err(AppError::UserNotFound);
            }

            // 5. Verify the storage key belongs to this user
            // This proves they have the password (since storageKey = SHA256(userId + password))
            let backups_table = write_txn.open_table(tables::BACKUPS)?;
            if let Some(backup_bytes) = backups_table.get(storage_key.as_str())? {
                let backup: BackupRecord = bincode::deserialize(backup_bytes.value())?;
                if backup.user_id != user_id {
                    tracing::warn!(
                        "Delete attempt with mismatched storage key for user: {}",
                        user_id
                    );
                    return Err(AppError::InvalidInput(
                        "Invalid credentials - storage key does not match user".to_string(),
                    ));
                }
            } else {
                tracing::warn!(
                    "Delete attempt with invalid storage key for user: {}",
                    user_id
                );
                return Err(AppError::InvalidInput(
                    "Invalid credentials - storage key does not match user".to_string(),
                ));
            }
            drop(backups_table);

            // 6. Get all backup keys for this user (for cascade delete)
            let mut user_backups = write_txn.open_table(tables::USER_BACKUPS)?;
            let backup_keys: Vec<String> = user_backups
                .get(user_id.as_str())?
                .map(|b| bincode::deserialize(b.value()).unwrap_or_default())
                .unwrap_or_default();

            // 7. Delete all backups
            let mut backups = write_txn.open_table(tables::BACKUPS)?;
            for key in &backup_keys {
                backups.remove(key.as_str())?;
            }
            drop(backups);

            // 8. Delete rate limits
            let mut rate_limits = write_txn.open_table(tables::RATE_LIMITS)?;
            rate_limits.remove(user_id.as_str())?;
            drop(rate_limits);

            // 9. Delete user_backups index
            user_backups.remove(user_id.as_str())?;
            drop(user_backups);

            // 10. Delete user
            users.remove(user_id.as_str())?;
        }
        write_txn.commit()?;

        tracing::info!(
            "User {} and all associated data successfully deleted",
            user_id
        );

        Ok(())
    })
    .await??;

    Ok(Json(DeleteUserResponse {
        success: true,
        message: "User and all associated data permanently deleted".to_string(),
    }))
}
