use axum::{extract::State, Json};
use serde_json::{json, Value};

use crate::AppState;

/// Health check endpoint
///
/// Returns the health status of the server and database connection.
/// Used by load balancers and monitoring systems.
pub async fn health_check(State(state): State<AppState>) -> Json<Value> {
    // Check database connectivity by attempting a read transaction
    let db = state.db.clone();
    let db_status = tokio::task::spawn_blocking(move || {
        match db.begin_read() {
            Ok(_) => "connected",
            Err(e) => {
                tracing::error!("Database health check failed: {:?}", e);
                "disconnected"
            }
        }
    })
    .await
    .unwrap_or("error");

    Json(json!({
        "status": if db_status == "connected" { "healthy" } else { "unhealthy" },
        "database": db_status,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
