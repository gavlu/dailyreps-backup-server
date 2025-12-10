use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Backup model representing encrypted user data
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Backup {
    /// Storage key (SHA-256 hash of userId + password)
    pub storage_key: String,
    /// User ID this backup belongs to
    pub user_id: String,
    /// Encrypted data blob (base64 encoded)
    pub encrypted_data: String,
    /// When the backup was created
    pub created_at: DateTime<Utc>,
    /// When the backup was last updated
    pub updated_at: DateTime<Utc>,
}

impl Backup {
    /// Validate that a storage key is a valid SHA-256 hash (64 hex characters)
    pub fn validate_storage_key(key: &str) -> bool {
        key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit())
    }

    /// Validate that encrypted data is valid base64
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
        let valid_key = "a" * 64;
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
}
