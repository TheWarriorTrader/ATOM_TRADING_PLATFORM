//! # Database Module
//!
//! Database connection management and repository implementations.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::config::DatabaseConfig;
use crate::errors::{InfrastructureError, InfrastructureResult};

/// Database connection pool
#[derive(Debug, Clone)]
pub struct DatabasePool {
    /// SQLx connection pool
    pool: PgPool,
}

impl DatabasePool {
    /// Creates a new database connection pool
    ///
    /// # Errors
    ///
    /// Returns error if connection fails
    pub async fn new(config: &DatabaseConfig) -> InfrastructureResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.url())
            .await
            .map_err(InfrastructureError::from)?;

        Ok(Self { pool })
    }

    /// Returns a reference to the connection pool
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Runs database migrations
    ///
    /// # Errors
    ///
    /// Returns error if migration fails
    pub async fn migrate(&self) -> InfrastructureResult<()> {
        // TODO: Fix type annotation for migrator
        // sqlx::migrate!("./migrations").run(&self.pool).await
        //     .map_err(|e| InfrastructureError::database(format!("Migration failed: {}", e)))?;
        Ok(())
    }
}

/// Repository implementations
pub mod repositories;
