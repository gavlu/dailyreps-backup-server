mod config;
mod constants;
mod db;
mod error;
mod models;
mod routes;
mod security;

use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use db::create_pool;
use routes::{delete_user, health_check, register_user, retrieve_backup, store_backup};

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub config: Config,
}

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

    // Create database connection pool
    let pool = create_pool(&config.database_url).await?;

    // Run migrations
    tracing::info!("Running database migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("Migrations complete");

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
        pool: pool.clone(),
        config: config.clone(),
    };

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/register", post(register_user))
        .route("/api/backup", post(store_backup).get(retrieve_backup))
        .route("/api/user", axum::routing::delete(delete_user))
        .layer(cors)
        .with_state(state);

    // Start server
    let addr: SocketAddr = config.server_address().parse()?;
    tracing::info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
