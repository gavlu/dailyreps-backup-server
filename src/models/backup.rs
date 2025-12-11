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

    /// Validate that encrypted data is valid base64
    ///
    /// Note: This is kept for compatibility with future compression analysis.
    /// The encrypted data from the client should be properly formatted base64
    /// containing JSON data that was encrypted with AES-GCM.
    pub fn validate_encrypted_data(data: &str) -> bool {
        // Basic check: base64 should only contain valid characters
        // More thorough validation would try to decode it
        !data.is_empty() && data.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='
        })
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
    fn test_validate_encrypted_data() {
        // Valid base64
        let valid_data = "SGVsbG8gV29ybGQ=";
        assert!(Backup::validate_encrypted_data(valid_data));

        // Empty
        assert!(!Backup::validate_encrypted_data(""));

        // Invalid base64 characters
        let invalid_data = "Hello@World!";
        assert!(!Backup::validate_encrypted_data(invalid_data));
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
        let bytes = bincode::serialize(&record).unwrap();
        let deserialized: BackupRecord = bincode::deserialize(&bytes).unwrap();

        assert_eq!(record.user_id, deserialized.user_id);
        assert_eq!(record.encrypted_data, deserialized.encrypted_data);
        assert_eq!(record.created_at, deserialized.created_at);
        assert_eq!(record.updated_at, deserialized.updated_at);
    }
}
