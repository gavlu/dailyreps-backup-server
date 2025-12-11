//! DailyReps Backup Server Library
//!
//! This module exports the core types and functions for testing and reuse.

pub mod config;
pub mod constants;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;
pub mod security;

pub use config::Config;
pub use db::{open_database, Db};
pub use error::{AppError, Result};

use std::sync::Arc;

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub config: Config,
}

impl AppState {
    /// Create a new AppState with the given database and configuration
    pub fn new(db: Arc<redb::Database>, config: Config) -> Self {
        Self { db, config }
    }
}
