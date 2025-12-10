use axum::{
    extract::{Query, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::constants::*;
use crate::error::{AppError, Result};
use crate::models::{Backup, User};
use crate::security::{validate_timestamp, verify_hmac};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct StoreBackupRequest {
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "storageKey")]
    pub storage_key: String,
    pub data: String,
    pub signature: String,
    pub timestamp: i64,
}

#[derive(Debug, Serialize)]
pub struct StoreBackupResponse {
    pub success: bool,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct RetrieveBackupParams {
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "storageKey")]
    pub storage_key: String,
}

#[derive(Debug, Serialize)]
pub struct RetrieveBackupResponse {
    pub data: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// Store or update encrypted backup
///
/// # Anti-Griefing Measures
/// 1. Size limit: Maximum 5MB payload
/// 2. HMAC signature: Proves data came from official app
/// 3. Timestamp validation: Prevents replay attacks
/// 4. Rate limiting: Max 5/hour, 20/day per user
pub async fn store_backup(
    State(state): State<AppState>,
    Json(payload): Json<StoreBackupRequest>,
) -> Result<Json<StoreBackupResponse>> {
    // 1. Verify HMAC signature (proves data from official app)
    if !verify_hmac(&payload.data, &payload.signature, &state.config.app_secret_key) {
        tracing::warn!(
            "Invalid HMAC signature from user {}",
            payload.user_id
        );
        return Err(AppError::InvalidSignature);
    }

    // 2. Validate timestamp (prevent replay attacks)
    if !validate_timestamp(payload.timestamp, MAX_TIMESTAMP_AGE_SECS) {
        return Err(AppError::InvalidInput(
            "Timestamp too old or in the future".to_string(),
        ));
    }

    // 3. Check payload size (prevent storage abuse)
    let payload_size = payload.data.len();
    if payload_size > MAX_BACKUP_SIZE_BYTES {
        tracing::warn!(
            "Payload too large from user {}: {} bytes (max: {})",
            payload.user_id,
            payload_size,
            MAX_BACKUP_SIZE_BYTES
        );
        return Err(AppError::PayloadTooLarge);
    }

    // Log warning for large payloads (monitoring)
    if payload_size > WARN_BACKUP_SIZE_BYTES {
        tracing::info!(
            "Large backup from user {}: {} bytes",
            payload.user_id,
            payload_size
        );
    }

    // 4. Validate user ID and storage key formats
    if !User::validate_id(&payload.user_id) {
        return Err(AppError::InvalidInput(
            "Invalid user ID format".to_string(),
        ));
    }

    if !Backup::validate_storage_key(&payload.storage_key) {
        return Err(AppError::InvalidInput(
            "Invalid storage key format".to_string(),
        ));
    }

    if !Backup::validate_encrypted_data(&payload.data) {
        return Err(AppError::InvalidInput(
            "Invalid encrypted data format".to_string(),
        ));
    }

    // 5. Verify user exists
    let user_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)"
    )
    .bind(&payload.user_id)
    .fetch_one(&state.pool)
    .await?;

    if !user_exists {
        tracing::warn!("Backup attempt for non-existent user: {}", payload.user_id);
        return Err(AppError::UserNotFound);
    }

    // 6. Check rate limits
    check_rate_limits(&state.pool, &payload.user_id).await?;

    // 7. Upsert backup (insert or update if exists)
    let now = Utc::now();
    sqlx::query!(
        r#"
        INSERT INTO backups (storage_key, user_id, encrypted_data, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $4)
        ON CONFLICT (storage_key)
        DO UPDATE SET
            encrypted_data = EXCLUDED.encrypted_data,
            updated_at = EXCLUDED.updated_at
        "#,
        payload.storage_key,
        payload.user_id,
        payload.data,
        now
    )
    .execute(&state.pool)
    .await?;

    // 8. Update rate limit counters
    update_rate_limits(&state.pool, &payload.user_id).await?;

    tracing::info!(
        "Backup stored for user {}: {} bytes",
        payload.user_id,
        payload_size
    );

    Ok(Json(StoreBackupResponse {
        success: true,
        updated_at: now.to_rfc3339(),
    }))
}

/// Retrieve encrypted backup
pub async fn retrieve_backup(
    State(state): State<AppState>,
    Query(params): Query<RetrieveBackupParams>,
) -> Result<Json<RetrieveBackupResponse>> {
    // Validate formats
    if !User::validate_id(&params.user_id) {
        return Err(AppError::InvalidInput(
            "Invalid user ID format".to_string(),
        ));
    }

    if !Backup::validate_storage_key(&params.storage_key) {
        return Err(AppError::InvalidInput(
            "Invalid storage key format".to_string(),
        ));
    }

    // Fetch backup
    let backup = sqlx::query_as!(
        Backup,
        r#"
        SELECT storage_key, user_id, encrypted_data, created_at, updated_at
        FROM backups
        WHERE storage_key = $1 AND user_id = $2
        "#,
        params.storage_key,
        params.user_id
    )
    .fetch_optional(&state.pool)
    .await?;

    match backup {
        Some(b) => {
            tracing::info!(
                "Backup retrieved for user {}: {} bytes",
                params.user_id,
                b.encrypted_data.len()
            );

            Ok(Json(RetrieveBackupResponse {
                data: b.encrypted_data,
                updated_at: b.updated_at.to_rfc3339(),
            }))
        }
        None => {
            tracing::info!(
                "Backup not found for user {} with storage_key {}",
                params.user_id,
                params.storage_key
            );
            Err(AppError::BackupNotFound)
        }
    }
}

/// Check if user has exceeded rate limits
async fn check_rate_limits(pool: &PgPool, user_id: &str) -> Result<()> {
    let now = Utc::now();

    // Get or create rate limit record
    let rate_limit = sqlx::query!(
        r#"
        SELECT backups_this_hour, backups_today, hour_reset_at, day_reset_at
        FROM user_rate_limits
        WHERE user_id = $1
        "#,
        user_id
    )
    .fetch_optional(pool)
    .await?;

    match rate_limit {
        Some(record) => {
            // Reset counters if time windows have expired
            let mut hour_count = record.backups_this_hour;
            let mut day_count = record.backups_today;

            if now > record.hour_reset_at {
                hour_count = 0;
            }

            if now > record.day_reset_at {
                day_count = 0;
            }

            // Check limits
            if hour_count >= MAX_BACKUPS_PER_HOUR {
                tracing::warn!(
                    "Hourly rate limit exceeded for user {}: {}/{}",
                    user_id,
                    hour_count,
                    MAX_BACKUPS_PER_HOUR
                );
                return Err(AppError::RateLimitExceeded);
            }

            if day_count >= MAX_BACKUPS_PER_DAY {
                tracing::warn!(
                    "Daily rate limit exceeded for user {}: {}/{}",
                    user_id,
                    day_count,
                    MAX_BACKUPS_PER_DAY
                );
                return Err(AppError::RateLimitExceeded);
            }

            Ok(())
        }
        None => {
            // First backup for this user - create record
            sqlx::query!(
                r#"
                INSERT INTO user_rate_limits (
                    user_id,
                    backups_this_hour,
                    backups_today,
                    hour_reset_at,
                    day_reset_at
                )
                VALUES ($1, 0, 0, $2, $3)
                "#,
                user_id,
                now + chrono::Duration::hours(1),
                now + chrono::Duration::days(1)
            )
            .execute(pool)
            .await?;

            Ok(())
        }
    }
}

/// Update rate limit counters after successful backup
async fn update_rate_limits(pool: &PgPool, user_id: &str) -> Result<()> {
    let now = Utc::now();

    sqlx::query!(
        r#"
        UPDATE user_rate_limits
        SET
            backups_this_hour = CASE
                WHEN $2 > hour_reset_at THEN 1
                ELSE backups_this_hour + 1
            END,
            backups_today = CASE
                WHEN $2 > day_reset_at THEN 1
                ELSE backups_today + 1
            END,
            hour_reset_at = CASE
                WHEN $2 > hour_reset_at THEN $2 + INTERVAL '1 hour'
                ELSE hour_reset_at
            END,
            day_reset_at = CASE
                WHEN $2 > day_reset_at THEN $2 + INTERVAL '1 day'
                ELSE day_reset_at
            END,
            last_backup_at = $2
        WHERE user_id = $1
        "#,
        user_id,
        now
    )
    .execute(pool)
    .await?;

    Ok(())
}
