use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::models::User;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    #[serde(rename = "userId")]
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub success: bool,
}

/// Register a new user
///
/// Creates a new user record with the provided user ID (SHA-256 hash of username).
/// Returns 409 Conflict if the user ID already exists.
///
/// # Anti-Griefing
/// - Stricter rate limiting applied at router level
/// - User ID must be valid SHA-256 hash format
pub async fn register_user(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>> {
    // Validate user ID format (must be 64-char hex string)
    if !User::validate_id(&payload.user_id) {
        tracing::warn!("Invalid user ID format: {}", payload.user_id);
        return Err(AppError::InvalidInput(
            "User ID must be a valid SHA-256 hash (64 hex characters)".to_string(),
        ));
    }

    // Check if user already exists
    let existing = sqlx::query_as!(
        User,
        "SELECT id, created_at FROM users WHERE id = $1",
        payload.user_id
    )
    .fetch_optional(&state.pool)
    .await?;

    if existing.is_some() {
        tracing::info!("User already exists: {}", payload.user_id);
        return Err(AppError::UserAlreadyExists);
    }

    // Insert new user
    sqlx::query!(
        "INSERT INTO users (id) VALUES ($1)",
        payload.user_id
    )
    .execute(&state.pool)
    .await?;

    tracing::info!("New user registered: {}", payload.user_id);

    Ok(Json(RegisterResponse { success: true }))
}
