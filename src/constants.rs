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

/// Expected app ID in backup envelope
pub const EXPECTED_APP_ID: &str = "dailyreps-app";

/// Current protocol version
pub const PROTOCOL_VERSION: &str = "1.0";
