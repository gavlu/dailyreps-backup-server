use axum::{
    extract::{Query, State},
    Json,
};
use chrono::Utc;
use redb::{ReadableDatabase, ReadableTable};
use serde::{Deserialize, Serialize};

const BINCODE_CONFIG: bincode::config::Configuration = bincode::config::standard();

use crate::constants::*;
use crate::db::tables;
use crate::error::{AppError, Result};
use crate::models::{Backup, BackupRecord, RateLimitRecord, User};
use crate::routes::{timestamp_to_rfc3339, validate_signed_request};
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
/// # Security Measures
/// 1. HMAC signature: Proves data came from official app
/// 2. Timestamp validation: Prevents replay attacks
/// 3. Rate limiting: Max 5/hour, 20/day per user
/// 4. Size limit: Maximum 5MB payload
pub async fn store_backup(
    State(state): State<AppState>,
    Json(payload): Json<StoreBackupRequest>,
) -> Result<Json<StoreBackupResponse>> {
    // 1. Verify HMAC signature and timestamp
    validate_signed_request(
        &payload.data,
        &payload.signature,
        payload.timestamp,
        &state.config.app_secret_key,
    )?;

    // 2. Check payload size
    let payload_size = payload.data.len();
    if payload_size > MAX_BACKUP_SIZE_BYTES {
        tracing::warn!(
            "Payload too large: {} bytes (max: {})",
            payload_size,
            MAX_BACKUP_SIZE_BYTES
        );
        return Err(AppError::PayloadTooLarge);
    }

    if payload_size > WARN_BACKUP_SIZE_BYTES {
        tracing::info!("Large backup: {} bytes", payload_size);
    }

    // 3. Validate user ID and storage key formats
    if !User::validate_id(&payload.user_id) {
        return Err(AppError::InvalidInput(ERR_INVALID_USER_ID.to_string()));
    }

    if !Backup::validate_storage_key(&payload.storage_key) {
        return Err(AppError::InvalidInput(ERR_INVALID_STORAGE_KEY.to_string()));
    }

    let db = state.db.clone();
    let user_id = payload.user_id.clone();
    let storage_key = payload.storage_key.clone();
    let data = payload.data.clone();

    let updated_at = tokio::task::spawn_blocking(move || -> Result<i64> {
        let now = Utc::now().timestamp();

        let write_txn = db.begin_write()?;
        {
            // 4. Verify user exists
            let users = write_txn.open_table(tables::USERS)?;
            if users.get(user_id.as_str())?.is_none() {
                tracing::warn!("Backup attempt for non-existent user");
                return Err(AppError::UserNotFound);
            }
            drop(users);

            // 5. Check and update rate limits
            let mut rate_limits = write_txn.open_table(tables::RATE_LIMITS)?;
            let mut rate_record = match rate_limits.get(user_id.as_str())? {
                Some(bytes) => {
                    let (record, _): (RateLimitRecord, _) =
                        bincode::serde::decode_from_slice(bytes.value(), BINCODE_CONFIG)?;
                    record
                }
                None => RateLimitRecord::new(now),
            };

            rate_record.check_and_increment(now)?;

            let rate_bytes = bincode::serde::encode_to_vec(&rate_record, BINCODE_CONFIG)?;
            rate_limits.insert(user_id.as_str(), rate_bytes.as_slice())?;
            drop(rate_limits);

            // 6. Upsert backup
            let mut backups = write_txn.open_table(tables::BACKUPS)?;
            let created_at = backups
                .get(storage_key.as_str())?
                .and_then(|b| {
                    bincode::serde::decode_from_slice::<BackupRecord, _>(b.value(), BINCODE_CONFIG)
                        .ok()
                        .map(|(r, _)| r)
                })
                .map(|r| r.created_at)
                .unwrap_or(now);

            let backup_record = BackupRecord {
                user_id: user_id.clone(),
                encrypted_data: data,
                created_at,
                updated_at: now,
            };
            let backup_bytes = bincode::serde::encode_to_vec(&backup_record, BINCODE_CONFIG)?;
            backups.insert(storage_key.as_str(), backup_bytes.as_slice())?;
            drop(backups);

            // 7. Update user_backups index
            let mut user_backups = write_txn.open_table(tables::USER_BACKUPS)?;
            let mut keys: Vec<String> = user_backups
                .get(user_id.as_str())?
                .and_then(|b| {
                    bincode::serde::decode_from_slice::<Vec<String>, _>(b.value(), BINCODE_CONFIG)
                        .ok()
                        .map(|(v, _)| v)
                })
                .unwrap_or_default();

            if !keys.contains(&storage_key) {
                keys.push(storage_key.clone());
                let keys_bytes = bincode::serde::encode_to_vec(&keys, BINCODE_CONFIG)?;
                user_backups.insert(user_id.as_str(), keys_bytes.as_slice())?;
            }
        }
        write_txn.commit()?;

        Ok(now)
    })
    .await??;

    tracing::info!("Backup stored: {} bytes", payload_size);

    Ok(Json(StoreBackupResponse {
        success: true,
        updated_at: timestamp_to_rfc3339(updated_at),
    }))
}

/// Retrieve encrypted backup
pub async fn retrieve_backup(
    State(state): State<AppState>,
    Query(params): Query<RetrieveBackupParams>,
) -> Result<Json<RetrieveBackupResponse>> {
    if !User::validate_id(&params.user_id) {
        return Err(AppError::InvalidInput(ERR_INVALID_USER_ID.to_string()));
    }

    if !Backup::validate_storage_key(&params.storage_key) {
        return Err(AppError::InvalidInput(ERR_INVALID_STORAGE_KEY.to_string()));
    }

    let db = state.db.clone();
    let user_id = params.user_id.clone();
    let storage_key = params.storage_key.clone();

    let result = tokio::task::spawn_blocking(move || -> Result<BackupRecord> {
        let read_txn = db.begin_read()?;
        let backups = read_txn.open_table(tables::BACKUPS)?;

        let record: BackupRecord = backups
            .get(storage_key.as_str())?
            .map(|b| {
                bincode::serde::decode_from_slice(b.value(), BINCODE_CONFIG)
                    .map(|(r, _)| r)
                    .map_err(AppError::from)
            })
            .transpose()?
            .ok_or_else(|| AppError::BackupNotFound)?;

        // Verify user_id matches
        if record.user_id != user_id {
            return Err(AppError::BackupNotFound);
        }

        Ok(record)
    })
    .await??;

    tracing::info!("Backup retrieved: {} bytes", result.encrypted_data.len());

    Ok(Json(RetrieveBackupResponse {
        data: result.encrypted_data,
        updated_at: timestamp_to_rfc3339(result.updated_at),
    }))
}
