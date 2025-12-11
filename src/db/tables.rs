use redb::TableDefinition;

/// Users table: user_id (SHA-256 hash) -> UserRecord (serialized)
pub const USERS: TableDefinition<&str, &[u8]> = TableDefinition::new("users");

/// Backups table: storage_key (SHA-256 hash) -> BackupRecord (serialized)
pub const BACKUPS: TableDefinition<&str, &[u8]> = TableDefinition::new("backups");

/// Rate limits table: user_id -> RateLimitRecord (serialized)
pub const RATE_LIMITS: TableDefinition<&str, &[u8]> = TableDefinition::new("rate_limits");

/// User backups index: user_id -> Vec<storage_key>
/// Used for cascade delete when a user is removed
pub const USER_BACKUPS: TableDefinition<&str, &[u8]> = TableDefinition::new("user_backups");
