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

// =============================================================================
// Compression Analysis Constants (Anomaly Detection)
// =============================================================================

/// Expected app identifier for envelope validation
/// Used to verify backups come from the official DailyReps app
pub const EXPECTED_APP_ID: &str = "dailyreps-app";

/// Minimum entropy ratio for valid encrypted JSON data
///
/// Encrypted data should have high entropy (close to 1.0). Data with low
/// entropy is suspicious because it suggests:
/// - Unencrypted data being uploaded
/// - Poorly compressed repetitive data
/// - Simple patterns that shouldn't be in properly encrypted data
///
/// We set 0.75 as a reasonable minimum - real AES-GCM encrypted data
/// typically has entropy between 0.95-1.0.
pub const MIN_ENTROPY_RATIO: f64 = 0.75;

/// Maximum entropy ratio for valid encrypted JSON data
///
/// Set to 1.0 (disabled) because properly encrypted data should have
/// entropy very close to 1.0. The envelope format validation (appId, version)
/// is the primary protection against random data abuse, not entropy checking.
///
/// Note: A strict upper bound (like 0.995) would incorrectly reject valid
/// encrypted data since AES-GCM produces near-perfect entropy.
pub const MAX_ENTROPY_RATIO: f64 = 1.0;

/// Minimum size for entropy analysis to be meaningful
/// Very small payloads don't have enough data for reliable entropy calculation
pub const MIN_SIZE_FOR_ENTROPY_CHECK: usize = 256;
