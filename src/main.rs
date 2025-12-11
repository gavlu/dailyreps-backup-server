use axum::{
    routing::{delete, get, post},
    Router,
};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use dailyreps_backup_server::{open_database, routes::*, AppState, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "dailyreps_backup_server=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting DailyReps Backup Server...");

    // Load configuration
    let config = Config::from_env().map_err(|e| anyhow::anyhow!(e))?;

    tracing::info!(
        "Environment: {}, Server: {}",
        config.environment,
        config.server_address()
    );

    // Open or create the embedded database
    let db = open_database(&config.database_path)?;

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(
            config
                .allowed_origins
                .iter()
                .map(|s| s.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::DELETE,
        ])
        .allow_headers(Any);

    // Create app state
    let state = AppState {
        db,
        config: config.clone(),
    };

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/register", post(register_user))
        .route("/api/backup", post(store_backup).get(retrieve_backup))
        .route("/api/user", delete(delete_user))
        .layer(cors)
        .with_state(state);

    // Start server
    let addr: SocketAddr = config.server_address().parse()?;
    tracing::info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
