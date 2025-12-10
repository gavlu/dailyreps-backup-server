use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Verify HMAC-SHA256 signature
///
/// This proves that the data came from the legitimate DailyReps app
/// and not from an arbitrary HTTP client trying to abuse storage.
///
/// # Arguments
/// * `data` - The data that was signed
/// * `signature` - The hex-encoded HMAC signature
/// * `secret` - The shared secret key (from environment)
///
/// # Security Note
/// The secret is hardcoded in the client app's JavaScript, so it can
/// be extracted by determined attackers. However, this significantly
/// raises the bar and prevents casual abuse.
pub fn verify_hmac(data: &str, signature: &str, secret: &str) -> bool {
    // Create HMAC instance with secret key
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => {
            tracing::error!("Failed to create HMAC instance");
            return false;
        }
    };

    // Update with data
    mac.update(data.as_bytes());

    // Decode hex signature
    let sig_bytes = match hex::decode(signature) {
        Ok(bytes) => bytes,
        Err(_) => {
            tracing::warn!("Invalid hex signature format");
            return false;
        }
    };

    // Verify signature
    mac.verify_slice(&sig_bytes).is_ok()
}

/// Validate timestamp is within acceptable range
///
/// Prevents replay attacks by ensuring the request is recent.
///
/// # Arguments
/// * `timestamp` - Unix timestamp in seconds from the client
/// * `max_age_secs` - Maximum age allowed in seconds
pub fn validate_timestamp(timestamp: i64, max_age_secs: i64) -> bool {
    let now = chrono::Utc::now().timestamp();
    let age_seconds = (now - timestamp).abs();

    if age_seconds > max_age_secs {
        tracing::warn!(
            "Timestamp too old: {} seconds (max: {})",
            age_seconds,
            max_age_secs
        );
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_hmac_valid() {
        let secret = "test-secret-key";
        let data = "test data";

        // Generate valid signature
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(data.as_bytes());
        let result = mac.finalize();
        let signature = hex::encode(result.into_bytes());

        // Should verify successfully
        assert!(verify_hmac(data, &signature, secret));
    }

    #[test]
    fn test_verify_hmac_invalid_signature() {
        let secret = "test-secret-key";
        let data = "test data";
        let wrong_signature = "0".repeat(64);

        assert!(!verify_hmac(data, &wrong_signature, secret));
    }

    #[test]
    fn test_verify_hmac_wrong_secret() {
        let secret = "test-secret-key";
        let wrong_secret = "wrong-secret";
        let data = "test data";

        // Generate signature with correct secret
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(data.as_bytes());
        let result = mac.finalize();
        let signature = hex::encode(result.into_bytes());

        // Verify with wrong secret should fail
        assert!(!verify_hmac(data, &signature, wrong_secret));
    }

    #[test]
    fn test_validate_timestamp_valid() {
        let now = chrono::Utc::now().timestamp();
        assert!(validate_timestamp(now, 300));
        assert!(validate_timestamp(now - 100, 300));
        assert!(validate_timestamp(now + 100, 300));
    }

    #[test]
    fn test_validate_timestamp_too_old() {
        let old = chrono::Utc::now().timestamp() - 400;
        assert!(!validate_timestamp(old, 300));
    }

    #[test]
    fn test_validate_timestamp_too_future() {
        let future = chrono::Utc::now().timestamp() + 400;
        assert!(!validate_timestamp(future, 300));
    }
}
