/// Maximum backup size in bytes (5MB)
/// Legitimate DailyReps data: ~300KB
/// This allows 16x headroom for growth
pub const MAX_BACKUP_SIZE_BYTES: usize = 5_242_880;

/// Warning threshold for large backups (1MB)
/// Log when backups exceed this size for monitoring
pub const WARN_BACKUP_SIZE_BYTES: usize = 1_048_576;

/// Maximum backup updates per hour per user
pub const MAX_BACKUPS_PER_HOUR: i32 = 5;

/// Maximum backup updates per day per user
pub const MAX_BACKUPS_PER_DAY: i32 = 20;

/// Maximum age of timestamp in seconds (5 minutes)
/// Prevents replay attacks
pub const MAX_TIMESTAMP_AGE_SECS: i64 = 300;

// =============================================================================
// Error Messages
// =============================================================================

/// Error message for invalid user ID format
pub const ERR_INVALID_USER_ID: &str = "Invalid user ID format";

/// Error message for invalid storage key format
pub const ERR_INVALID_STORAGE_KEY: &str = "Invalid storage key format";

/// Error message for timestamp validation failure
pub const ERR_INVALID_TIMESTAMP: &str = "Timestamp too old or in the future";

/// Detailed error message for user ID validation in registration
pub const ERR_USER_ID_MUST_BE_SHA256: &str =
    "User ID must be a valid SHA-256 hash (64 hex characters)";
