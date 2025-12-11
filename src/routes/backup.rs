use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

use crate::constants::*;
use crate::db::tables;
use crate::error::{AppError, Result};
use crate::models::{Backup, BackupRecord, RateLimitRecord, User};
use crate::security::{analyze_backup_data, validate_timestamp, verify_hmac};
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
/// 5. Compression analysis: Detects anomalous data patterns (entropy check)
///
/// # Compression Analysis (Anomaly Detection)
/// Analyzes the backup envelope format and entropy of encrypted data to detect:
/// - Non-official clients (missing/wrong appId)
/// - Random data padding (entropy too high)
/// - Unencrypted data (entropy too low)
pub async fn store_backup(
    State(state): State<AppState>,
    Json(payload): Json<StoreBackupRequest>,
) -> Result<Json<StoreBackupResponse>> {
    // 1. Verify HMAC signature (proves data from official app)
    if !verify_hmac(
        &payload.data,
        &payload.signature,
        &state.config.app_secret_key,
    ) {
        tracing::warn!("Invalid HMAC signature from user {}", payload.user_id);
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
        return Err(AppError::InvalidInput("Invalid user ID format".to_string()));
    }

    if !Backup::validate_storage_key(&payload.storage_key) {
        return Err(AppError::InvalidInput(
            "Invalid storage key format".to_string(),
        ));
    }

    // 5. Compression analysis (anomaly detection)
    // Validates envelope format and checks entropy of encrypted data
    let analysis = analyze_backup_data(&payload.data).map_err(AppError::InvalidBackupFormat)?;

    // Log entropy analysis results
    if analysis.data_size >= MIN_SIZE_FOR_ENTROPY_CHECK {
        tracing::debug!(
            "Backup entropy for user {}: {:.3} (size: {} bytes)",
            payload.user_id,
            analysis.entropy_ratio,
            analysis.data_size
        );
    }

    // Reject backups that fail entropy check (suspicious patterns)
    if !analysis.passes_entropy_check {
        tracing::warn!(
            "Rejecting backup from user {} due to suspicious entropy: {:.3}",
            payload.user_id,
            analysis.entropy_ratio
        );
        return Err(AppError::SuspiciousDataPattern);
    }

    // Log any warnings (for monitoring)
    if let Some(warning) = &analysis.warning {
        tracing::info!("Backup warning for user {}: {}", payload.user_id, warning);
    }

    let db = state.db.clone();
    let user_id = payload.user_id.clone();
    let storage_key = payload.storage_key.clone();
    let data = payload.data.clone();

    let updated_at = tokio::task::spawn_blocking(move || -> Result<i64> {
        let now = Utc::now().timestamp();

        let write_txn = db.begin_write()?;
        {
            // 6. Verify user exists
            let users = write_txn.open_table(tables::USERS)?;
            if users.get(user_id.as_str())?.is_none() {
                tracing::warn!("Backup attempt for non-existent user: {}", user_id);
                return Err(AppError::UserNotFound);
            }
            drop(users);

            // 7. Check and update rate limits
            let mut rate_limits = write_txn.open_table(tables::RATE_LIMITS)?;
            let mut rate_record = match rate_limits.get(user_id.as_str())? {
                Some(bytes) => bincode::deserialize(bytes.value())?,
                None => RateLimitRecord::new(now),
            };

            // This will return Err(RateLimitExceeded) if limits are exceeded
            rate_record.check_and_increment(now)?;

            let rate_bytes = bincode::serialize(&rate_record)?;
            rate_limits.insert(user_id.as_str(), rate_bytes.as_slice())?;
            drop(rate_limits);

            // 8. Upsert backup (insert or update if exists)
            let mut backups = write_txn.open_table(tables::BACKUPS)?;
            let created_at = backups
                .get(storage_key.as_str())?
                .and_then(|b| bincode::deserialize::<BackupRecord>(b.value()).ok())
                .map(|r| r.created_at)
                .unwrap_or(now);

            let backup_record = BackupRecord {
                user_id: user_id.clone(),
                encrypted_data: data,
                created_at,
                updated_at: now,
            };
            let backup_bytes = bincode::serialize(&backup_record)?;
            backups.insert(storage_key.as_str(), backup_bytes.as_slice())?;
            drop(backups);

            // 9. Update user_backups index (for cascade delete)
            let mut user_backups = write_txn.open_table(tables::USER_BACKUPS)?;
            let mut keys: Vec<String> = user_backups
                .get(user_id.as_str())?
                .map(|b| bincode::deserialize(b.value()).unwrap_or_default())
                .unwrap_or_default();

            if !keys.contains(&storage_key) {
                keys.push(storage_key.clone());
                let keys_bytes = bincode::serialize(&keys)?;
                user_backups.insert(user_id.as_str(), keys_bytes.as_slice())?;
            }
        }
        write_txn.commit()?;

        Ok(now)
    })
    .await??;

    tracing::info!(
        "Backup stored for user {}: {} bytes",
        payload.user_id,
        payload_size
    );

    let updated_at_dt = DateTime::from_timestamp(updated_at, 0).unwrap_or_else(Utc::now);

    Ok(Json(StoreBackupResponse {
        success: true,
        updated_at: updated_at_dt.to_rfc3339(),
    }))
}

/// Retrieve encrypted backup
pub async fn retrieve_backup(
    State(state): State<AppState>,
    Query(params): Query<RetrieveBackupParams>,
) -> Result<Json<RetrieveBackupResponse>> {
    // Validate formats
    if !User::validate_id(&params.user_id) {
        return Err(AppError::InvalidInput("Invalid user ID format".to_string()));
    }

    if !Backup::validate_storage_key(&params.storage_key) {
        return Err(AppError::InvalidInput(
            "Invalid storage key format".to_string(),
        ));
    }

    let db = state.db.clone();
    let storage_key = params.storage_key.clone();
    let user_id = params.user_id.clone();

    let result = tokio::task::spawn_blocking(move || -> Result<BackupRecord> {
        let read_txn = db.begin_read()?;
        let backups = read_txn.open_table(tables::BACKUPS)?;

        let record: BackupRecord = backups
            .get(storage_key.as_str())?
            .map(|b| bincode::deserialize(b.value()))
            .transpose()?
            .ok_or_else(|| AppError::BackupNotFound)?;

        // Verify user_id matches (security check)
        if record.user_id != user_id {
            return Err(AppError::BackupNotFound);
        }

        Ok(record)
    })
    .await??;

    tracing::info!(
        "Backup retrieved for user {}: {} bytes",
        params.user_id,
        result.encrypted_data.len()
    );

    let updated_at_dt = DateTime::from_timestamp(result.updated_at, 0).unwrap_or_else(Utc::now);

    Ok(Json(RetrieveBackupResponse {
        data: result.encrypted_data,
        updated_at: updated_at_dt.to_rfc3339(),
    }))
}
