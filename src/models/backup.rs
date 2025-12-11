use serde::{Deserialize, Serialize};

/// Backup record stored in redb
/// Uses Unix timestamps for compact storage with bincode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRecord {
    /// User ID this backup belongs to
    pub user_id: String,
    /// Encrypted data blob (base64 encoded from client)
    pub encrypted_data: String,
    /// When the backup was created (Unix timestamp)
    pub created_at: i64,
    /// When the backup was last updated (Unix timestamp)
    pub updated_at: i64,
}

/// Backup model for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    /// Storage key (SHA-256 hash of userId + password)
    pub storage_key: String,
    /// User ID this backup belongs to
    pub user_id: String,
    /// Encrypted data blob (base64 encoded)
    pub encrypted_data: String,
    /// When the backup was created (Unix timestamp)
    pub created_at: i64,
    /// When the backup was last updated (Unix timestamp)
    pub updated_at: i64,
}

impl Backup {
    /// Validate that a storage key is a valid SHA-256 hash (64 hex characters)
    pub fn validate_storage_key(key: &str) -> bool {
        key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_storage_key() {
        // Valid SHA-256 hash
        let valid_key = "a".repeat(64);
        assert!(Backup::validate_storage_key(&valid_key));

        // Invalid length
        let invalid_key = "abc123";
        assert!(!Backup::validate_storage_key(&invalid_key));
    }

    #[test]
    fn test_backup_record_serialization() {
        let record = BackupRecord {
            user_id: "a".repeat(64),
            encrypted_data: "SGVsbG8gV29ybGQ=".to_string(),
            created_at: 1733788800,
            updated_at: 1733788800,
        };

        // Verify bincode serialization works
        let config = bincode::config::standard();
        let bytes = bincode::serde::encode_to_vec(&record, config).unwrap();
        let (deserialized, _): (BackupRecord, _) =
            bincode::serde::decode_from_slice(&bytes, config).unwrap();

        assert_eq!(record.user_id, deserialized.user_id);
        assert_eq!(record.encrypted_data, deserialized.encrypted_data);
        assert_eq!(record.created_at, deserialized.created_at);
        assert_eq!(record.updated_at, deserialized.updated_at);
    }
}
