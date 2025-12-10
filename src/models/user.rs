use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// User model representing a registered user
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    /// User ID (SHA-256 hash of username)
    pub id: String,
    /// When the user was created
    pub created_at: DateTime<Utc>,
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
        let valid_id = "a" * 64;
        assert!(User::validate_id(&valid_id));

        // Too short
        let short_id = "abc123";
        assert!(!User::validate_id(&short_id));

        // Too long
        let long_id = "a" * 65;
        assert!(!User::validate_id(&long_id));

        // Invalid characters
        let invalid_id = "z".repeat(64);
        assert!(!User::validate_id(&invalid_id));

        // Real SHA-256 hash
        let real_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(User::validate_id(real_hash));
    }
}
