//! # Infrastructure Errors
//!
//! Error types for infrastructure layer operations.

use thiserror::Error;

/// Result type alias for infrastructure operations
pub type InfrastructureResult<T> = Result<T, InfrastructureError>;

/// Errors that can occur in infrastructure operations
#[derive(Error, Debug)]
pub enum InfrastructureError {
    /// Database error
    #[error("Database error: {0}")]
    Database(String),

    /// Connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// Query error
    #[error("Query error: {0}")]
    Query(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// External API error
    #[error("External API error: {0}")]
    ExternalApi(String),

    /// Cache error
    #[error("Cache error: {0}")]
    Cache(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Timeout
    #[error("Timeout: {0}")]
    Timeout(String),
}

impl InfrastructureError {
    /// Creates a database error
    #[must_use]
    pub fn database(msg: impl Into<String>) -> Self {
        Self::Database(msg.into())
    }

    /// Creates a connection error
    #[must_use]
    pub fn connection(msg: impl Into<String>) -> Self {
        Self::Connection(msg.into())
    }

    /// Creates a query error
    #[must_use]
    pub fn query(msg: impl Into<String>) -> Self {
        Self::Query(msg.into())
    }

    /// Creates an external API error
    #[must_use]
    pub fn external_api(msg: impl Into<String>) -> Self {
        Self::ExternalApi(msg.into())
    }

    /// Creates a cache error
    #[must_use]
    pub fn cache(msg: impl Into<String>) -> Self {
        Self::Cache(msg.into())
    }

    /// Creates a configuration error
    #[must_use]
    pub fn configuration(msg: impl Into<String>) -> Self {
        Self::Configuration(msg.into())
    }
}

impl From<sqlx::Error> for InfrastructureError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound("Record not found".to_string()),
            sqlx::Error::PoolTimedOut => Self::Timeout("Database connection timeout".to_string()),
            sqlx::Error::PoolClosed => Self::Connection("Connection pool closed".to_string()),
            _ => Self::Database(err.to_string()),
        }
    }
}

impl From<redis::RedisError> for InfrastructureError {
    fn from(err: redis::RedisError) -> Self {
        Self::Cache(err.to_string())
    }
}

impl From<reqwest::Error> for InfrastructureError {
    fn from(err: reqwest::Error) -> Self {
        Self::ExternalApi(err.to_string())
    }
}

impl From<serde_json::Error> for InfrastructureError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}
