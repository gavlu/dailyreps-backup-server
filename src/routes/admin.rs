use axum::{
    Json,
    extract::{Query, State},
};
use redb::{ReadableDatabase, ReadableTableMetadata};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::{AppError, AppState, db::tables, error::Result};

/// Query parameters for admin stats endpoint
#[derive(Debug, Deserialize)]
pub struct AdminQuery {
    /// Admin secret key for authentication
    pub key: String,
}

/// Database statistics response
#[derive(Debug, Serialize)]
pub struct AdminStatsResponse {
    pub user_count: u64,
    pub backup_count: u64,
    pub database_size_bytes: u64,
    pub database_size_human: String,
}

/// Format bytes into human-readable string
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Admin stats endpoint
///
/// Returns database statistics for monitoring and diagnostics.
/// Requires admin secret key passed as query parameter.
///
/// GET /admin/stats?key=<admin_secret_key>
pub async fn admin_stats(
    State(state): State<AppState>,
    Query(params): Query<AdminQuery>,
) -> Result<Json<AdminStatsResponse>> {
    // Check if admin endpoints are enabled
    let admin_key = state
        .config
        .admin_secret_key
        .as_ref()
        .ok_or(AppError::Unauthorized)?;

    // Verify the provided key matches
    if params.key != *admin_key {
        tracing::warn!("Invalid admin key attempt");
        return Err(AppError::Unauthorized);
    }

    // Get database file size
    let db_path = state.config.database_path.clone();
    let database_size_bytes = fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    // Count records in database
    let db = state.db.clone();
    let (user_count, backup_count) = tokio::task::spawn_blocking(move || -> Result<(u64, u64)> {
        let read_txn = db.begin_read()?;

        let user_count = match read_txn.open_table(tables::USERS) {
            Ok(table) => table.len()?,
            Err(_) => 0,
        };

        let backup_count = match read_txn.open_table(tables::BACKUPS) {
            Ok(table) => table.len()?,
            Err(_) => 0,
        };

        Ok((user_count, backup_count))
    })
    .await??;

    tracing::info!(
        "Admin stats requested: {} users, {} backups, {} database",
        user_count,
        backup_count,
        format_bytes(database_size_bytes)
    );

    Ok(Json(AdminStatsResponse {
        user_count,
        backup_count,
        database_size_bytes,
        database_size_human: format_bytes(database_size_bytes),
    }))
}
