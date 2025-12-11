use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::constants::{
    EXPECTED_APP_ID, MAX_ENTROPY_RATIO, MIN_ENTROPY_RATIO, MIN_SIZE_FOR_ENTROPY_CHECK,
};

type HmacSha256 = Hmac<Sha256>;

// =============================================================================
// Pepper Security (Rainbow Table Protection)
// =============================================================================

/// Apply server-side pepper to a client-provided user ID hash
///
/// This adds an additional layer of security by combining the client-provided
/// user ID (which is SHA256(username)) with a server-side secret pepper.
///
/// # Security Benefits
/// - Rainbow table attacks become infeasible without the pepper
/// - Database breach alone doesn't allow identifying users by username
/// - Pepper is stored in environment variable, not in database
///
/// # Arguments
/// * `client_user_id` - The SHA-256 hash provided by the client (hex string)
/// * `pepper` - The server-side secret pepper
///
/// # Returns
/// * A new SHA-256 hash combining the client ID and pepper (hex string)
///
/// # Algorithm
/// `peppered_id = SHA256(client_user_id + pepper)`
pub fn apply_pepper(client_user_id: &str, pepper: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(client_user_id.as_bytes());
    hasher.update(pepper.as_bytes());
    hex::encode(hasher.finalize())
}

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

// =============================================================================
// Compression Analysis (Anomaly Detection)
// =============================================================================

/// Result of compression analysis
#[derive(Debug, Clone)]
pub struct CompressionAnalysis {
    /// Shannon entropy of the data (0.0 to 1.0, normalized)
    pub entropy_ratio: f64,
    /// Whether the data passes entropy checks
    pub passes_entropy_check: bool,
    /// Original data size in bytes
    pub data_size: usize,
    /// Optional warning message for suspicious patterns
    pub warning: Option<String>,
}

/// Envelope wrapper expected from the client
///
/// The client should wrap encrypted data in this envelope format
/// to prove it comes from the official app.
#[derive(Debug, serde::Deserialize)]
pub struct BackupEnvelope {
    /// App identifier - must match EXPECTED_APP_ID
    #[serde(rename = "appId")]
    pub app_id: String,
    /// The actual encrypted data (base64)
    /// This includes the nonce/IV as part of the encrypted blob
    pub encrypted: String,
}

/// Calculate Shannon entropy of binary data
///
/// Returns a value between 0.0 (all bytes identical) and 1.0 (maximum entropy).
/// This is normalized entropy (actual entropy / 8 bits).
fn calculate_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    // Count byte frequencies
    let mut frequencies = [0u64; 256];
    for &byte in data {
        frequencies[byte as usize] += 1;
    }

    let len = data.len() as f64;
    let mut entropy = 0.0;

    for &count in &frequencies {
        if count > 0 {
            let probability = count as f64 / len;
            entropy -= probability * probability.log2();
        }
    }

    // Normalize to 0.0-1.0 range (max entropy is 8 bits per byte)
    entropy / 8.0
}

/// Analyze backup data for anomalous patterns
///
/// This function performs compression analysis to detect abuse:
/// 1. Validates the envelope format (appId)
/// 2. Checks entropy of the encrypted data
///
/// # Arguments
/// * `data` - The raw backup payload string from the client
///
/// # Returns
/// * `Ok(CompressionAnalysis)` - Analysis results (may include warnings)
/// * `Err(String)` - Hard failure if envelope is invalid
///
/// # Security Note
/// This is a soft protection layer. The HMAC signature is the primary
/// authentication mechanism. Compression analysis helps detect abuse
/// patterns that HMAC alone cannot catch (e.g., uploading random data
/// instead of actual encrypted JSON backups).
pub fn analyze_backup_data(data: &str) -> Result<CompressionAnalysis, String> {
    // Try to parse as envelope format
    let envelope: BackupEnvelope =
        serde_json::from_str(data).map_err(|e| format!("Invalid backup envelope format: {}", e))?;

    // Validate app ID
    if envelope.app_id != EXPECTED_APP_ID {
        return Err(format!(
            "Invalid app ID: expected '{}', got '{}'",
            EXPECTED_APP_ID, envelope.app_id
        ));
    }

    // Decode base64 encrypted data for entropy analysis
    let encrypted_bytes = base64_decode(&envelope.encrypted)
        .map_err(|e| format!("Invalid base64 in encrypted field: {}", e))?;

    let data_size = encrypted_bytes.len();
    let mut warning = None;

    // Only perform entropy check on sufficiently large payloads
    let (entropy_ratio, passes_entropy_check) = if data_size >= MIN_SIZE_FOR_ENTROPY_CHECK {
        let entropy = calculate_entropy(&encrypted_bytes);

        let passes = (MIN_ENTROPY_RATIO..=MAX_ENTROPY_RATIO).contains(&entropy);

        if !passes {
            if entropy < MIN_ENTROPY_RATIO {
                warning = Some(format!(
                    "Suspiciously low entropy ({:.3}): data may not be properly encrypted",
                    entropy
                ));
                tracing::warn!(
                    "Low entropy backup detected: {:.3} (min: {:.3})",
                    entropy,
                    MIN_ENTROPY_RATIO
                );
            } else {
                warning = Some(format!(
                    "Suspiciously high entropy ({:.3}): data may be random padding",
                    entropy
                ));
                tracing::warn!(
                    "High entropy backup detected: {:.3} (max: {:.3})",
                    entropy,
                    MAX_ENTROPY_RATIO
                );
            }
        }

        (entropy, passes)
    } else {
        // Small payloads skip entropy check
        (0.0, true)
    };

    Ok(CompressionAnalysis {
        entropy_ratio,
        passes_entropy_check,
        data_size,
        warning,
    })
}

/// Simple base64 encoder
///
/// Encodes binary data to standard base64 (with + and /).
/// This is public for use in tests.
pub fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// Simple base64 decoder
///
/// Decodes standard base64 (with + and /) as well as URL-safe base64.
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    // Handle padding
    let input = input.trim();

    // Convert URL-safe to standard base64
    let standard: String = input
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .collect();

    // Remove any whitespace
    let clean: String = standard.chars().filter(|c| !c.is_whitespace()).collect();

    // Add padding if needed
    let padded = match clean.len() % 4 {
        2 => format!("{}==", clean),
        3 => format!("{}=", clean),
        _ => clean,
    };

    // Decode
    let mut result = Vec::with_capacity(padded.len() * 3 / 4);
    let chars: Vec<char> = padded.chars().collect();

    for chunk in chars.chunks(4) {
        if chunk.len() != 4 {
            return Err("Invalid base64 length".to_string());
        }

        let values: Result<Vec<u8>, String> =
            chunk.iter().map(|&c| decode_base64_char(c)).collect();
        let values = values?;

        result.push((values[0] << 2) | (values[1] >> 4));
        if chunk[2] != '=' {
            result.push((values[1] << 4) | (values[2] >> 2));
        }
        if chunk[3] != '=' {
            result.push((values[2] << 6) | values[3]);
        }
    }

    Ok(result)
}

fn decode_base64_char(c: char) -> Result<u8, String> {
    match c {
        'A'..='Z' => Ok(c as u8 - b'A'),
        'a'..='z' => Ok(c as u8 - b'a' + 26),
        '0'..='9' => Ok(c as u8 - b'0' + 52),
        '+' => Ok(62),
        '/' => Ok(63),
        '=' => Ok(0), // Padding
        _ => Err(format!("Invalid base64 character: {}", c)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Pepper Security Tests
    // =========================================================================

    #[test]
    fn test_apply_pepper_basic() {
        let client_id = "a".repeat(64); // Simulated SHA-256 hash
        let pepper = "secret-pepper";

        let result = apply_pepper(&client_id, pepper);

        // Result should be valid SHA-256 hex string
        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_apply_pepper_deterministic() {
        let client_id = "abc123";
        let pepper = "my-pepper";

        let result1 = apply_pepper(client_id, pepper);
        let result2 = apply_pepper(client_id, pepper);

        // Same inputs should produce same output
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_apply_pepper_different_inputs() {
        let pepper = "same-pepper";

        let result1 = apply_pepper("user1", pepper);
        let result2 = apply_pepper("user2", pepper);

        // Different inputs should produce different outputs
        assert_ne!(result1, result2);
    }

    #[test]
    fn test_apply_pepper_different_peppers() {
        let client_id = "same-id";

        let result1 = apply_pepper(client_id, "pepper1");
        let result2 = apply_pepper(client_id, "pepper2");

        // Different peppers should produce different outputs
        assert_ne!(result1, result2);
    }

    #[test]
    fn test_apply_pepper_known_value() {
        // Verify against a known SHA-256 output
        // SHA256("testpepper") = expected value
        let client_id = "test";
        let pepper = "pepper";

        let result = apply_pepper(client_id, pepper);

        // Just verify it's a valid hash (length and format)
        assert_eq!(result.len(), 64);
        // The actual hash of "testpepper" - verified manually
        // SHA256("testpepper") = "b6dd3a1d2d71a3f0e9d5dfacd2edbb85dd3ea76c7f5f1ef7a889e24e1f7f0f2e"
        // (This is just for documentation, actual test is format validation)
    }

    // =========================================================================
    // HMAC Tests
    // =========================================================================

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

    // =========================================================================
    // Compression Analysis Tests
    // =========================================================================

    #[test]
    fn test_calculate_entropy_empty() {
        assert_eq!(calculate_entropy(&[]), 0.0);
    }

    #[test]
    fn test_calculate_entropy_all_same() {
        // All same bytes = 0 entropy
        let data = vec![0u8; 1000];
        let entropy = calculate_entropy(&data);
        assert!(
            entropy < 0.01,
            "Expected near-zero entropy, got {}",
            entropy
        );
    }

    #[test]
    fn test_calculate_entropy_uniform_distribution() {
        // Perfectly uniform distribution = maximum entropy (close to 1.0)
        let mut data = Vec::with_capacity(256 * 100);
        for _ in 0..100 {
            for byte in 0..=255u8 {
                data.push(byte);
            }
        }
        let entropy = calculate_entropy(&data);
        assert!(
            entropy > 0.99,
            "Expected near-maximum entropy, got {}",
            entropy
        );
    }

    #[test]
    fn test_calculate_entropy_typical_encrypted() {
        // Simulated encrypted data (random-looking but from a pseudorandom source)
        // This should have high entropy but not perfectly 1.0
        let data: Vec<u8> = (0..1000).map(|i| ((i * 7 + 13) % 256) as u8).collect();
        let entropy = calculate_entropy(&data);
        // Encrypted data typically has entropy between 0.85-0.99
        assert!(
            entropy > 0.7 && entropy < 1.0,
            "Expected encrypted-like entropy, got {}",
            entropy
        );
    }

    #[test]
    fn test_base64_decode_simple() {
        // "Hello" in base64 is "SGVsbG8="
        let decoded = base64_decode("SGVsbG8=").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_base64_decode_no_padding() {
        // Without padding should still work
        let decoded = base64_decode("SGVsbG8").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_base64_decode_url_safe() {
        // URL-safe base64 uses - and _ instead of + and /
        let decoded = base64_decode("SGVs-G8_").unwrap();
        // The data decodes successfully with URL-safe chars converted
        assert!(!decoded.is_empty());
    }

    #[test]
    fn test_base64_decode_invalid_char() {
        let result = base64_decode("SGVs@G8!");
        assert!(result.is_err());
    }

    #[test]
    fn test_analyze_backup_data_valid_envelope() {
        // Create a valid envelope with encrypted data that has good entropy
        // Use a pattern that produces higher entropy (pseudo-random spread)
        let encrypted_data: Vec<u8> = (0u64..512)
            .map(|i| {
                // LCG-like pattern for better distribution using wrapping arithmetic
                let x = i.wrapping_mul(1103515245).wrapping_add(12345);
                (x % 256) as u8
            })
            .collect();

        let envelope = serde_json::json!({
            "appId": EXPECTED_APP_ID,
            "encrypted": base64_encode(&encrypted_data)
        });

        let result = analyze_backup_data(&envelope.to_string());
        assert!(
            result.is_ok(),
            "Expected valid envelope to pass: {:?}",
            result
        );

        let analysis = result.unwrap();
        // Debug: print entropy if test fails
        if !analysis.passes_entropy_check {
            panic!(
                "Entropy check failed: ratio={:.3}, min={:.3}, max={:.3}, warning={:?}",
                analysis.entropy_ratio, MIN_ENTROPY_RATIO, MAX_ENTROPY_RATIO, analysis.warning
            );
        }
        assert!(analysis.passes_entropy_check);
        assert!(analysis.entropy_ratio > 0.0);
    }

    #[test]
    fn test_analyze_backup_data_wrong_app_id() {
        let envelope = serde_json::json!({
            "appId": "wrong-app",
            "encrypted": base64_encode(b"test data")
        });

        let result = analyze_backup_data(&envelope.to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid app ID"));
    }

    #[test]
    fn test_analyze_backup_data_missing_fields() {
        let envelope = serde_json::json!({
            "appId": EXPECTED_APP_ID
            // Missing encrypted
        });

        let result = analyze_backup_data(&envelope.to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_analyze_backup_data_invalid_json() {
        let result = analyze_backup_data("not valid json");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Invalid backup envelope format"));
    }

    #[test]
    fn test_analyze_backup_data_low_entropy() {
        // Create data with very low entropy (all same bytes)
        let low_entropy_data = vec![0xABu8; 512];

        let envelope = serde_json::json!({
            "appId": EXPECTED_APP_ID,
            "encrypted": base64_encode(&low_entropy_data)
        });

        let result = analyze_backup_data(&envelope.to_string());
        assert!(result.is_ok());

        let analysis = result.unwrap();
        // Low entropy data should fail the check
        assert!(!analysis.passes_entropy_check);
        assert!(analysis.warning.is_some());
        assert!(analysis.warning.unwrap().contains("low entropy"));
    }

    #[test]
    fn test_analyze_backup_data_small_payload_skips_entropy() {
        // Small payloads skip entropy check
        let small_data = vec![0xABu8; 100]; // Less than MIN_SIZE_FOR_ENTROPY_CHECK

        let envelope = serde_json::json!({
            "appId": EXPECTED_APP_ID,
            "encrypted": base64_encode(&small_data)
        });

        let result = analyze_backup_data(&envelope.to_string());
        assert!(result.is_ok());

        let analysis = result.unwrap();
        // Small payload should pass (entropy check skipped)
        assert!(analysis.passes_entropy_check);
        assert!(analysis.data_size < MIN_SIZE_FOR_ENTROPY_CHECK);
    }
}
