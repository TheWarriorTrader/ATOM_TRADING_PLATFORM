//! # Cache Module
//!
//! Redis and in-memory cache implementations.

use redis::{aio::ConnectionManager, Client};

use crate::config::RedisConfig;
use crate::errors::{InfrastructureError, InfrastructureResult};

/// Redis cache client
#[derive(Clone)]
pub struct RedisCache {
    /// Redis connection manager
    connection: ConnectionManager,
}

impl std::fmt::Debug for RedisCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisCache")
            .field("connection", &"<ConnectionManager>")
            .finish()
    }
}

impl RedisCache {
    /// Creates a new Redis cache client
    ///
    /// # Errors
    ///
    /// Returns error if connection fails
    pub async fn new(config: &RedisConfig) -> InfrastructureResult<Self> {
        let client = Client::open(config.url())
            .map_err(|e| InfrastructureError::cache(format!("Failed to create client: {}", e)))?;

        let connection = client
            .get_connection_manager()
            .await
            .map_err(|e| InfrastructureError::cache(format!("Failed to get connection: {}", e)))?;

        Ok(Self { connection })
    }

    /// Gets a value from cache
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub async fn get(&self, key: &str) -> InfrastructureResult<Option<String>> {
        let mut conn = self.connection.clone();
        let value: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(InfrastructureError::from)?;
        Ok(value)
    }

    /// Sets a value in cache
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub async fn set(&self, key: &str, value: &str, ttl_seconds: u64) -> InfrastructureResult<()> {
        let mut conn = self.connection.clone();
        redis::cmd("SETEX")
            .arg(key)
            .arg(ttl_seconds)
            .arg(value)
            .query_async::<_, ()>(&mut conn)
            .await
            .map_err(InfrastructureError::from)?;
        Ok(())
    }

    /// Deletes a key from cache
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub async fn delete(&self, key: &str) -> InfrastructureResult<()> {
        let mut conn = self.connection.clone();
        redis::cmd("DEL")
            .arg(key)
            .query_async::<_, ()>(&mut conn)
            .await
            .map_err(InfrastructureError::from)?;
        Ok(())
    }

    /// Checks if a key exists
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub async fn exists(&self, key: &str) -> InfrastructureResult<bool> {
        let mut conn = self.connection.clone();
        let exists: bool = redis::cmd("EXISTS")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(InfrastructureError::from)?;
        Ok(exists)
    }
}
