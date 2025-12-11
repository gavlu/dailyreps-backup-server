pub mod tables;

use redb::{Database, Error as RedbError};
use std::path::Path;
use std::sync::Arc;

/// Database handle type (Arc-wrapped for sharing across handlers)
pub type Db = Arc<Database>;

/// Open or create the redb database at the given path
///
/// Creates all required tables on first run.
#[allow(clippy::result_large_err)]
pub fn open_database(path: impl AsRef<Path>) -> Result<Db, RedbError> {
    tracing::info!("Opening database at: {:?}", path.as_ref());

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.as_ref().parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            tracing::error!("Failed to create database directory: {}", e);
            RedbError::Io(e)
        })?;
    }

    let db = Database::create(path)?;

    // Initialize tables on first run
    let write_txn = db.begin_write()?;
    {
        // Create tables if they don't exist by opening them
        let _ = write_txn.open_table(tables::USERS)?;
        let _ = write_txn.open_table(tables::BACKUPS)?;
        let _ = write_txn.open_table(tables::RATE_LIMITS)?;
        let _ = write_txn.open_table(tables::USER_BACKUPS)?;
    }
    write_txn.commit()?;

    tracing::info!("Database initialized successfully");

    Ok(Arc::new(db))
}
