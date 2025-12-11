use chrono::{DateTime, Utc};

use crate::constants::{ERR_INVALID_TIMESTAMP, MAX_TIMESTAMP_AGE_SECS};
use crate::error::AppError;
use crate::security::{validate_timestamp, verify_hmac};

/// Convert Unix timestamp to RFC3339 string, defaulting to now if invalid
pub fn timestamp_to_rfc3339(timestamp: i64) -> String {
    DateTime::from_timestamp(timestamp, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

/// Error type for signed request validation (constrained to only possible errors)
#[derive(Debug)]
pub enum SignedRequestError {
    InvalidSignature,
    InvalidTimestamp,
}

impl From<SignedRequestError> for AppError {
    fn from(err: SignedRequestError) -> Self {
        match err {
            SignedRequestError::InvalidSignature => AppError::InvalidSignature,
            SignedRequestError::InvalidTimestamp => {
                AppError::InvalidInput(ERR_INVALID_TIMESTAMP.to_string())
            }
        }
    }
}

/// Verify HMAC signature and timestamp for authenticated requests
pub fn validate_signed_request(
    data: &str,
    signature: &str,
    timestamp: i64,
    secret: &str,
) -> Result<(), SignedRequestError> {
    if !verify_hmac(data, signature, secret) {
        tracing::warn!("Invalid HMAC signature");
        return Err(SignedRequestError::InvalidSignature);
    }

    if !validate_timestamp(timestamp, MAX_TIMESTAMP_AGE_SECS) {
        return Err(SignedRequestError::InvalidTimestamp);
    }

    Ok(())
}
