use std::env;

/// Application configuration loaded from environment variables
#[derive(Debug, Clone)]
pub struct Config {
    pub server_host: String,
    pub server_port: u16,
    pub database_path: String,
    pub allowed_origins: Vec<String>,
    pub rate_limit_requests: u64,
    pub rate_limit_window_secs: u64,
    pub register_rate_limit_requests: u64,
    pub register_rate_limit_window_secs: u64,
    pub environment: String,
    pub app_secret_key: String,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, String> {
        // Load .env file if it exists (development)
        dotenvy::dotenv().ok();

        let server_host = env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let server_port = env::var("SERVER_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .map_err(|_| "Invalid SERVER_PORT")?;

        let database_path =
            env::var("DATABASE_PATH").unwrap_or_else(|_| "./data/dailyreps.db".to_string());

        let allowed_origins = env::var("ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:5173".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        let rate_limit_requests = env::var("RATE_LIMIT_REQUESTS")
            .unwrap_or_else(|_| "100".to_string())
            .parse()
            .map_err(|_| "Invalid RATE_LIMIT_REQUESTS")?;

        let rate_limit_window_secs = env::var("RATE_LIMIT_WINDOW_SECS")
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .map_err(|_| "Invalid RATE_LIMIT_WINDOW_SECS")?;

        let register_rate_limit_requests = env::var("REGISTER_RATE_LIMIT_REQUESTS")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .map_err(|_| "Invalid REGISTER_RATE_LIMIT_REQUESTS")?;

        let register_rate_limit_window_secs = env::var("REGISTER_RATE_LIMIT_WINDOW_SECS")
            .unwrap_or_else(|_| "300".to_string())
            .parse()
            .map_err(|_| "Invalid REGISTER_RATE_LIMIT_WINDOW_SECS")?;

        let environment = env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string());

        let app_secret_key = env::var("APP_SECRET_KEY")
            .map_err(|_| "APP_SECRET_KEY must be set for HMAC verification")?;

        Ok(Config {
            server_host,
            server_port,
            database_path,
            allowed_origins,
            rate_limit_requests,
            rate_limit_window_secs,
            register_rate_limit_requests,
            register_rate_limit_window_secs,
            environment,
            app_secret_key,
        })
    }

    /// Get server address as string
    pub fn server_address(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}
