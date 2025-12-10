use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::models::User;
use crate::security::{validate_timestamp, verify_hmac};
use crate::constants::MAX_TIMESTAMP_AGE_SECS;
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
///
/// # Security
/// - Requires HMAC signature verification
/// - Requires timestamp validation
/// - Validates user ID and storage key formats
/// - Cascading delete removes all associated data (via FK constraints)
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

    // 4. Verify user exists
    let user_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)"
    )
    .bind(&payload.user_id)
    .fetch_one(&state.pool)
    .await?;

    if !user_exists {
        tracing::warn!("Delete attempt for non-existent user: {}", payload.user_id);
        return Err(AppError::UserNotFound);
    }

    // 5. Verify the storage key belongs to this user
    // This proves they have the password (since storageKey = SHA256(userId + password))
    let backup_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM backups WHERE user_id = $1 AND storage_key = $2)"
    )
    .bind(&payload.user_id)
    .bind(&payload.storage_key)
    .fetch_one(&state.pool)
    .await?;

    if !backup_exists {
        tracing::warn!(
            "Delete attempt with invalid storage key for user: {}",
            payload.user_id
        );
        return Err(AppError::InvalidInput(
            "Invalid credentials - storage key does not match user".to_string(),
        ));
    }

    // 6. Delete user (cascading delete will remove backups and rate limits)
    let result = sqlx::query!(
        "DELETE FROM users WHERE id = $1",
        payload.user_id
    )
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        tracing::error!(
            "User {} existed but delete affected 0 rows",
            payload.user_id
        );
        return Err(AppError::InternalError);
    }

    tracing::info!(
        "User {} and all associated data successfully deleted",
        payload.user_id
    );

    Ok(Json(DeleteUserResponse {
        success: true,
        message: "User and all associated data permanently deleted".to_string(),
    }))
}
