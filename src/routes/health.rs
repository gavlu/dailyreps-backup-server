use axum::{extract::State, Json};
use serde_json::{json, Value};

use crate::AppState;

/// Health check endpoint
///
/// Returns the health status of the server and database connection.
/// Used by load balancers and monitoring systems.
pub async fn health_check(State(state): State<AppState>) -> Json<Value> {
    // Check database connectivity
    let db_status = match sqlx::query("SELECT 1").fetch_one(&state.pool).await {
        Ok(_) => "connected",
        Err(e) => {
            tracing::error!("Database health check failed: {:?}", e);
            "disconnected"
        }
    };

    Json(json!({
        "status": if db_status == "connected" { "healthy" } else { "unhealthy" },
        "database": db_status,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
