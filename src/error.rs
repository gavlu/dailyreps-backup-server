use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// Application error type
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("User already exists")]
    UserAlreadyExists,

    #[error("User not found")]
    UserNotFound,

    #[error("Backup not found")]
    BackupNotFound,

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Payload too large")]
    PayloadTooLarge,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Internal server error")]
    InternalError,
}

/// Implement IntoResponse to convert AppError into HTTP responses
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Database(ref e) => {
                // Log the actual database error server-side
                tracing::error!("Database error: {:?}", e);
                // Return generic error to client
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
            AppError::UserAlreadyExists => (StatusCode::CONFLICT, "User already exists"),
            AppError::UserNotFound => (StatusCode::UNAUTHORIZED, "User not found"),
            AppError::BackupNotFound => (StatusCode::NOT_FOUND, "Backup not found"),
            AppError::InvalidInput(ref msg) => (StatusCode::BAD_REQUEST, msg.as_str()),
            AppError::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "Backup size exceeds maximum allowed",
            ),
            AppError::InvalidSignature => (
                StatusCode::UNAUTHORIZED,
                "Invalid signature - data must come from official app",
            ),
            AppError::RateLimitExceeded => (
                StatusCode::TOO_MANY_REQUESTS,
                "Rate limit exceeded - too many requests",
            ),
            AppError::InternalError => {
                tracing::error!("Internal error occurred");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
        };

        let body = Json(json!({
            "error": error_message
        }));

        (status, body).into_response()
    }
}

/// Result type alias for application results
pub type Result<T> = std::result::Result<T, AppError>;
