use serde::{Deserialize, Serialize};

/// User record stored in redb
/// Uses Unix timestamp for compact storage with bincode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    /// When the user was created (Unix timestamp)
    pub created_at: i64,
}

/// User model for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// User ID (SHA-256 hash of username)
    pub id: String,
    /// When the user was created (Unix timestamp)
    pub created_at: i64,
}

impl User {
    /// Validate that a user ID is a valid SHA-256 hash (64 hex characters)
    pub fn validate_id(id: &str) -> bool {
        id.len() == 64 && id.chars().all(|c| c.is_ascii_hexdigit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_id() {
        // Valid SHA-256 hash (64 hex characters)
        let valid_id = "a".repeat(64);
        assert!(User::validate_id(&valid_id));

        // Too short
        let short_id = "abc123";
        assert!(!User::validate_id(&short_id));

        // Too long
        let long_id = "a".repeat(65);
        assert!(!User::validate_id(&long_id));

        // Invalid characters
        let invalid_id = "z".repeat(64);
        assert!(!User::validate_id(&invalid_id));

        // Real SHA-256 hash
        let real_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(User::validate_id(real_hash));
    }

    #[test]
    fn test_user_record_serialization() {
        let record = UserRecord {
            created_at: 1733788800,
        };

        // Verify bincode serialization works
        let bytes = bincode::serialize(&record).unwrap();
        let deserialized: UserRecord = bincode::deserialize(&bytes).unwrap();

        assert_eq!(record.created_at, deserialized.created_at);
    }
}
