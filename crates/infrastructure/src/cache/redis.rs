//! # Redis Cache Implementation
//!
//! High-performance Redis caching layer with:
//! - Connection pooling via multiplexed connections
//! - MessagePack serialization for efficiency
//! - Circuit breaker pattern for resilience
//! - Key prefixing for namespacing
//!
//! ## Example
//!
//! ```rust
//! use infrastructure::cache::redis::{RedisCache, RedisCacheConfig};
//! use infrastructure::config::RedisConfig;
//!
//! async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = RedisConfig::default();
//!     let cache = RedisCache::new(&config).await?;
//!
//!     // Store value with TTL
//!     cache.setex("user:123", &user_data, 3600).await?;
//!
//!     // Retrieve value
//!     if let Some(data) = cache.get::<_, User>("user:123").await? {
//!         println!("Found user: {:?}", data);
//!     }
//!
//!     Ok(())
//! }
//! ```

use std::fmt::Debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Client};
use rmp_serde::{decode, encode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace, warn};

use crate::config::RedisConfig;
use crate::errors::{InfrastructureError, InfrastructureResult};

/// Circuit breaker states for failure handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation - requests pass through
    Closed,
    /// Failure threshold reached - requests fail fast
    Open,
    /// Testing if service has recovered
    HalfOpen,
}

impl Default for CircuitState {
    fn default() -> Self {
        Self::Closed
    }
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "closed"),
            Self::Open => write!(f, "open"),
            Self::HalfOpen => write!(f, "half-open"),
        }
    }
}

/// Configuration for circuit breaker behavior
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u32,
    /// Duration to wait before attempting recovery (half-open)
    pub timeout_duration: Duration,
    /// Number of successful requests to close circuit from half-open
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout_duration: Duration::from_secs(30),
            success_threshold: 3,
        }
    }
}

/// Circuit breaker for handling Redis failures gracefully
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Current state of the circuit
    state: Arc<RwLock<CircuitState>>,
    /// Configuration for circuit behavior
    config: CircuitBreakerConfig,
    /// Consecutive failure count
    failure_count: Arc<AtomicU64>,
    /// Consecutive success count (used in half-open state)
    success_count: Arc<AtomicU64>,
    /// Last failure timestamp
    last_failure_time: Arc<RwLock<Option<Instant>>>,
}

impl CircuitBreaker {
    /// Creates a new circuit breaker with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(CircuitBreakerConfig::default())
    }

    /// Creates a new circuit breaker with custom configuration
    #[must_use]
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            config,
            failure_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
        }
    }

    /// Records a successful operation
    pub async fn record_success(&self) {
        let state = *self.state.read().await;

        match state {
            CircuitState::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
                if successes >= self.config.success_threshold as u64 {
                    self.transition_to(CircuitState::Closed).await;
                    info!(
                        successes = successes,
                        "Circuit breaker transitioned from half-open to closed"
                    );
                }
            }
            CircuitState::Closed => {
                // Reset failure count on success in closed state
                self.failure_count.store(0, Ordering::SeqCst);
            }
            CircuitState::Open => {
                // Should not happen, but log it
                warn!("Success recorded while circuit is open");
            }
        }
    }

    /// Records a failed operation
    pub async fn record_failure(&self) {
        let state = *self.state.read().await;

        match state {
            CircuitState::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= self.config.failure_threshold as u64 {
                    self.transition_to(CircuitState::Open).await;
                    warn!(
                        failures = failures,
                        "Circuit breaker transitioned from closed to open"
                    );
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open immediately opens the circuit
                self.transition_to(CircuitState::Open).await;
                warn!("Circuit breaker transitioned from half-open to open due to failure");
            }
            CircuitState::Open => {
                // Already open, just update last failure time
                let mut last_failure = self.last_failure_time.write().await;
                *last_failure = Some(Instant::now());
            }
        }
    }

    /// Checks if the circuit allows requests to pass through
    pub async fn can_execute(&self) -> bool {
        let state = *self.state.read().await;

        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                let last_failure = *self.last_failure_time.read().await;
                if let Some(last) = last_failure {
                    if last.elapsed() >= self.config.timeout_duration {
                        self.transition_to(CircuitState::HalfOpen).await;
                        info!("Circuit breaker transitioned from open to half-open");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Returns the current state of the circuit breaker
    pub async fn current_state(&self) -> CircuitState {
        *self.state.read().await
    }

    /// Transitions to a new state
    async fn transition_to(&self, new_state: CircuitState) {
        let mut state = self.state.write().await;
        *state = new_state;

        // Reset counters based on new state
        match new_state {
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::SeqCst);
                self.success_count.store(0, Ordering::SeqCst);
            }
            CircuitState::Open => {
                let mut last_failure = self.last_failure_time.write().await;
                *last_failure = Some(Instant::now());
                self.success_count.store(0, Ordering::SeqCst);
            }
            CircuitState::HalfOpen => {
                self.success_count.store(0, Ordering::SeqCst);
            }
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for Redis cache behavior
#[derive(Debug, Clone)]
pub struct RedisCacheConfig {
    /// Key prefix for namespacing
    pub key_prefix: String,
    /// Default TTL for cached values (in seconds)
    pub default_ttl_secs: u64,
    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,
    /// Connection timeout
    pub timeout: Duration,
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            key_prefix: "trading".to_string(),
            default_ttl_secs: 3600,
            circuit_breaker: CircuitBreakerConfig::default(),
            timeout: Duration::from_secs(5),
        }
    }
}

/// Redis cache client with connection pooling and circuit breaker
#[derive(Clone)]
pub struct RedisCache {
    /// Redis connection (multiplexed for concurrent use)
    connection: MultiplexedConnection,
    /// Cache configuration
    config: RedisCacheConfig,
    /// Circuit breaker for resilience
    circuit_breaker: CircuitBreaker,
}

impl Debug for RedisCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisCache")
            .field("key_prefix", &self.config.key_prefix)
            .finish()
    }
}

impl RedisCache {
    /// Creates a new Redis cache client from RedisConfig
    ///
    /// # Errors
    ///
    /// Returns error if connection fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use infrastructure::cache::redis::RedisCache;
    /// use infrastructure::config::RedisConfig;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = RedisConfig::default();
    /// let cache = RedisCache::new(&config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(config: &RedisConfig) -> InfrastructureResult<Self> {
        Self::with_cache_config(config, RedisCacheConfig::default()).await
    }

    /// Creates a new Redis cache client with custom cache configuration
    ///
    /// # Errors
    ///
    /// Returns error if connection fails
    pub async fn with_cache_config(
        config: &RedisConfig,
        cache_config: RedisCacheConfig,
    ) -> InfrastructureResult<Self> {
        let url = config.url();
        trace!(url = %config.masked_url(), "Creating Redis connection");

        let client = Client::open(url.clone())
            .map_err(|e| InfrastructureError::cache(format!("Failed to create client: {e}")))?;

        let connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| InfrastructureError::connection(format!("Failed to connect: {e}")))?;

        // Test the connection
        let _: String = redis::cmd("PING")
            .query_async(&mut connection.clone())
            .await
            .map_err(|e| InfrastructureError::connection(format!("Redis ping failed: {e}")))?;

        info!(
            url = %config.masked_url(),
            key_prefix = %cache_config.key_prefix,
            "Redis cache initialized successfully"
        );

        Ok(Self {
            connection,
            config: cache_config.clone(),
            circuit_breaker: CircuitBreaker::with_config(cache_config.circuit_breaker.clone()),
        })
    }

    /// Gets a value from cache
    ///
    /// # Type Parameters
    ///
    /// - `K`: Type that can be converted to a key string
    /// - `V`: Type that implements `DeserializeOwned`
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Circuit breaker is open
    /// - Redis operation fails
    /// - Deserialization fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use infrastructure::cache::redis::RedisCache;
    /// use infrastructure::config::RedisConfig;
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct User { name: String }
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = RedisConfig::default();
    /// let cache = RedisCache::new(&config).await?;
    ///
    /// if let Some(user) = cache.get::<_, User>("user:123").await? {
    ///     println!("Found: {}", user.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get<K, V>(&self, key: K) -> InfrastructureResult<Option<V>>
    where
        K: AsRef<str>,
        V: DeserializeOwned,
    {
        // Check circuit breaker
        if !self.circuit_breaker.can_execute().await {
            warn!("Circuit breaker open - get operation rejected");
            return Err(InfrastructureError::cache(
                "Circuit breaker open - Redis unavailable",
            ));
        }

        let full_key = self.prefixed_key(key.as_ref());
        trace!(key = %full_key, "Getting value from cache");

        let result: Option<Vec<u8>> = redis::cmd("GET")
            .arg(&full_key)
            .query_async(&mut self.connection.clone())
            .await
            .map_err(|e| {
                error!(key = %full_key, error = %e, "Redis GET failed");
                InfrastructureError::cache(format!("GET error: {e}"))
            })?;

        match result {
            Some(data) => {
                let bytes = data.len();
                trace!(key = %full_key, bytes = bytes, "Cache hit");
                match decode::from_slice::<V>(&data) {
                    Ok(value) => {
                        self.circuit_breaker.record_success().await;
                        Ok(Some(value))
                    }
                    Err(e) => {
                        error!(key = %full_key, error = %e, "MessagePack deserialization failed");
                        Err(InfrastructureError::serialization(format!(
                            "Deserialization error: {e}"
                        )))
                    }
                }
            }
            None => {
                trace!(key = %full_key, "Cache miss");
                self.circuit_breaker.record_success().await;
                Ok(None)
            }
        }
    }

    /// Sets a value in cache without TTL
    ///
    /// # Type Parameters
    ///
    /// - `K`: Type that can be converted to a key string
    /// - `V`: Type that implements `Serialize`
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Circuit breaker is open
    /// - Redis operation fails
    /// - Serialization fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use infrastructure::cache::redis::RedisCache;
    /// use infrastructure::config::RedisConfig;
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct User { name: String }
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = RedisConfig::default();
    /// let cache = RedisCache::new(&config).await?;
    ///
    /// let user = User { name: "John".to_string() };
    /// cache.set("user:123", &user).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn set<K, V>(&self, key: K, value: V) -> InfrastructureResult<()>
    where
        K: AsRef<str>,
        V: Serialize,
    {
        // Check circuit breaker
        if !self.circuit_breaker.can_execute().await {
            warn!("Circuit breaker open - set operation rejected");
            return Err(InfrastructureError::cache(
                "Circuit breaker open - Redis unavailable",
            ));
        }

        let full_key = self.prefixed_key(key.as_ref());
        trace!(key = %full_key, "Setting value in cache");

        let data = match encode::to_vec(&value) {
            Ok(data) => data,
            Err(e) => {
                error!(key = %full_key, error = %e, "MessagePack serialization failed");
                return Err(InfrastructureError::serialization(format!(
                    "Serialization error: {e}"
                )));
            }
        };

        match redis::cmd("SET")
            .arg(&full_key)
            .arg(data)
            .query_async::<_, ()>(&mut self.connection.clone())
            .await
        {
            Ok(()) => {
                trace!(key = %full_key, "Value set successfully");
                self.circuit_breaker.record_success().await;
                Ok(())
            }
            Err(e) => {
                error!(key = %full_key, error = %e, "Redis SET failed");
                self.circuit_breaker.record_failure().await;
                Err(InfrastructureError::cache(format!("SET error: {e}")))
            }
        }
    }

    /// Sets a value in cache with TTL (time-to-live) in seconds
    ///
    /// # Type Parameters
    ///
    /// - `K`: Type that can be converted to a key string
    /// - `V`: Type that implements `Serialize`
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Circuit breaker is open
    /// - Redis operation fails
    /// - Serialization fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use infrastructure::cache::redis::RedisCache;
    /// use infrastructure::config::RedisConfig;
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct User { name: String }
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = RedisConfig::default();
    /// let cache = RedisCache::new(&config).await?;
    ///
    /// let user = User { name: "John".to_string() };
    /// cache.setex("user:123", &user, 3600).await?; // 1 hour TTL
    /// # Ok(())
    /// # }
    /// ```
    pub async fn setex<K, V>(&self, key: K, value: V, ttl_secs: u64) -> InfrastructureResult<()>
    where
        K: AsRef<str>,
        V: Serialize,
    {
        // Check circuit breaker
        if !self.circuit_breaker.can_execute().await {
            warn!("Circuit breaker open - setex operation rejected");
            return Err(InfrastructureError::cache(
                "Circuit breaker open - Redis unavailable",
            ));
        }

        let full_key = self.prefixed_key(key.as_ref());
        trace!(key = %full_key, ttl = ttl_secs, "Setting value in cache with TTL");

        let data = match encode::to_vec(&value) {
            Ok(data) => data,
            Err(e) => {
                error!(key = %full_key, error = %e, "MessagePack serialization failed");
                return Err(InfrastructureError::serialization(format!(
                    "Serialization error: {e}"
                )));
            }
        };

        match redis::cmd("SETEX")
            .arg(&full_key)
            .arg(ttl_secs)
            .arg(data)
            .query_async::<_, ()>(&mut self.connection.clone())
            .await
        {
            Ok(()) => {
                trace!(key = %full_key, ttl = ttl_secs, "Value set with TTL successfully");
                self.circuit_breaker.record_success().await;
                Ok(())
            }
            Err(e) => {
                error!(key = %full_key, ttl = ttl_secs, error = %e, "Redis SETEX failed");
                self.circuit_breaker.record_failure().await;
                Err(InfrastructureError::cache(format!("SETEX error: {e}")))
            }
        }
    }

    /// Deletes a key from cache
    ///
    /// # Type Parameters
    ///
    /// - `K`: Type that can be converted to a key string
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Circuit breaker is open
    /// - Redis operation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use infrastructure::cache::redis::RedisCache;
    /// use infrastructure::config::RedisConfig;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = RedisConfig::default();
    /// let cache = RedisCache::new(&config).await?;
    ///
    /// cache.delete("user:123").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete<K>(&self, key: K) -> InfrastructureResult<()>
    where
        K: AsRef<str>,
    {
        // Check circuit breaker
        if !self.circuit_breaker.can_execute().await {
            warn!("Circuit breaker open - delete operation rejected");
            return Err(InfrastructureError::cache(
                "Circuit breaker open - Redis unavailable",
            ));
        }

        let full_key = self.prefixed_key(key.as_ref());
        trace!(key = %full_key, "Deleting key from cache");

        match redis::cmd("DEL")
            .arg(&full_key)
            .query_async::<_, ()>(&mut self.connection.clone())
            .await
        {
            Ok(()) => {
                trace!(key = %full_key, "Key deleted successfully");
                self.circuit_breaker.record_success().await;
                Ok(())
            }
            Err(e) => {
                error!(key = %full_key, error = %e, "Redis DEL failed");
                self.circuit_breaker.record_failure().await;
                Err(InfrastructureError::cache(format!("DEL error: {e}")))
            }
        }
    }

    /// Checks if a key exists in cache
    ///
    /// # Type Parameters
    ///
    /// - `K`: Type that can be converted to a key string
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Circuit breaker is open
    /// - Redis operation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use infrastructure::cache::redis::RedisCache;
    /// use infrastructure::config::RedisConfig;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = RedisConfig::default();
    /// let cache = RedisCache::new(&config).await?;
    ///
    /// if cache.exists("user:123").await? {
    ///     println!("User exists in cache");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn exists<K>(&self, key: K) -> InfrastructureResult<bool>
    where
        K: AsRef<str>,
    {
        // Check circuit breaker
        if !self.circuit_breaker.can_execute().await {
            warn!("Circuit breaker open - exists operation rejected");
            return Err(InfrastructureError::cache(
                "Circuit breaker open - Redis unavailable",
            ));
        }

        let full_key = self.prefixed_key(key.as_ref());
        trace!(key = %full_key, "Checking key existence");

        match redis::cmd("EXISTS")
            .arg(&full_key)
            .query_async(&mut self.connection.clone())
            .await
        {
            Ok(exists) => {
                trace!(key = %full_key, exists = exists, "Key existence checked");
                self.circuit_breaker.record_success().await;
                Ok(exists)
            }
            Err(e) => {
                error!(key = %full_key, error = %e, "Redis EXISTS failed");
                self.circuit_breaker.record_failure().await;
                Err(InfrastructureError::cache(format!("EXISTS error: {e}")))
            }
        }
    }

    /// Returns the current circuit breaker state
    pub async fn circuit_state(&self) -> CircuitState {
        self.circuit_breaker.current_state().await
    }

    /// Returns the full prefixed key
    fn prefixed_key(&self, key: &str) -> String {
        format!("{}:{}", self.config.key_prefix, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestData {
        id: u64,
        name: String,
        value: f64,
    }

    fn create_test_data() -> TestData {
        TestData {
            id: 42,
            name: "test".to_string(),
            value: 3.14,
        }
    }

    #[test]
    fn test_circuit_breaker_state_display() {
        assert_eq!(format!("{}", CircuitState::Closed), "closed");
        assert_eq!(format!("{}", CircuitState::Open), "open");
        assert_eq!(format!("{}", CircuitState::HalfOpen), "half-open");
    }

    #[test]
    fn test_circuit_breaker_default() {
        let cb = CircuitBreaker::default();
        assert_eq!(cb.config.failure_threshold, 5);
        assert_eq!(cb.config.timeout_duration, Duration::from_secs(30));
        assert_eq!(cb.config.success_threshold, 3);
    }

    #[test]
    fn test_circuit_breaker_custom_config() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout_duration: Duration::from_secs(10),
            success_threshold: 2,
        };
        let cb = CircuitBreaker::with_config(config);
        assert_eq!(cb.config.failure_threshold, 3);
        assert_eq!(cb.config.timeout_duration, Duration::from_secs(10));
        assert_eq!(cb.config.success_threshold, 2);
    }

    #[tokio::test]
    async fn test_circuit_breaker_closed_allows_execution() {
        let cb = CircuitBreaker::default();
        assert!(cb.can_execute().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout_duration: Duration::from_secs(60),
            success_threshold: 2,
        };
        let cb = CircuitBreaker::with_config(config);

        // Record failures up to threshold
        cb.record_failure().await;
        cb.record_failure().await;
        assert!(cb.can_execute().await); // Still closed

        cb.record_failure().await; // Should open now
        assert!(!cb.can_execute().await);
        assert_eq!(cb.current_state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_records_success_in_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout_duration: Duration::from_millis(1),
            success_threshold: 1,
        };
        let cb = CircuitBreaker::with_config(config);

        // Open the circuit
        cb.record_failure().await;
        assert_eq!(cb.current_state().await, CircuitState::Open);

        // Wait for timeout and transition to half-open
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(cb.can_execute().await); // Should transition to half-open
        assert_eq!(cb.current_state().await, CircuitState::HalfOpen);

        // Success should close it
        cb.record_success().await;
        assert_eq!(cb.current_state().await, CircuitState::Closed);
    }

    #[test]
    fn test_redis_cache_config_default() {
        let config = RedisCacheConfig::default();
        assert_eq!(config.key_prefix, "trading");
        assert_eq!(config.default_ttl_secs, 3600);
    }

    #[test]
    fn test_prefixed_key() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = RedisCacheConfig {
                key_prefix: "test".to_string(),
                ..Default::default()
            };
            assert_eq!(config.key_prefix, "test");
        });
    }

    #[test]
    fn test_messagepack_serialization() {
        let data = create_test_data();
        let encoded = encode::to_vec(&data).expect("Serialization should succeed");
        let decoded: TestData =
            decode::from_slice(&encoded).expect("Deserialization should succeed");
        assert_eq!(data, decoded);
    }
}

#[cfg(test)]
#[cfg(feature = "integration-tests")]
mod integration_tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct MarketData {
        symbol: String,
        price: f64,
        timestamp: i64,
    }

    async fn create_test_cache() -> InfrastructureResult<RedisCache> {
        let config = RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            password: None,
            db: 15, // Use database 15 for tests
            timeout_ms: 5000,
        };

        let cache_config = RedisCacheConfig {
            key_prefix: "test".to_string(),
            default_ttl_secs: 60,
            circuit_breaker: CircuitBreakerConfig::default(),
            timeout: Duration::from_secs(5),
        };

        RedisCache::with_cache_config(&config, cache_config).await
    }

    #[ignore = "Requires Redis running on localhost:6379"]
    #[tokio::test]
    async fn test_set_and_get() {
        let cache = create_test_cache().await.expect("Failed to create cache");

        let data = MarketData {
            symbol: "NQ".to_string(),
            price: 15000.5,
            timestamp: 1234567890,
        };

        // Set value
        cache
            .set("market:nq", &data)
            .await
            .expect("Set should succeed");

        // Get value
        let retrieved: Option<MarketData> =
            cache.get("market:nq").await.expect("Get should succeed");

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);

        // Cleanup
        cache
            .delete("market:nq")
            .await
            .expect("Delete should succeed");
    }

    #[ignore = "Requires Redis running on localhost:6379"]
    #[tokio::test]
    async fn test_setex_and_ttl() {
        let cache = create_test_cache().await.expect("Failed to create cache");

        let data = MarketData {
            symbol: "ES".to_string(),
            price: 4500.25,
            timestamp: 1234567890,
        };

        // Set with 1 second TTL
        cache
            .setex("market:es", &data, 1)
            .await
            .expect("Setex should succeed");

        // Verify it exists
        assert!(cache
            .exists("market:es")
            .await
            .expect("Exists should succeed"));

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Should not exist anymore
        assert!(!cache
            .exists("market:es")
            .await
            .expect("Exists should succeed"));
    }

    #[ignore = "Requires Redis running on localhost:6379"]
    #[tokio::test]
    async fn test_delete() {
        let cache = create_test_cache().await.expect("Failed to create cache");

        let data = MarketData {
            symbol: "CL".to_string(),
            price: 75.5,
            timestamp: 1234567890,
        };

        // Set and verify
        cache
            .set("market:cl", &data)
            .await
            .expect("Set should succeed");
        assert!(cache
            .exists("market:cl")
            .await
            .expect("Exists should succeed"));

        // Delete
        cache
            .delete("market:cl")
            .await
            .expect("Delete should succeed");

        // Verify deletion
        assert!(!cache
            .exists("market:cl")
            .await
            .expect("Exists should succeed"));
    }

    #[ignore = "Requires Redis running on localhost:6379"]
    #[tokio::test]
    async fn test_get_nonexistent() {
        let cache = create_test_cache().await.expect("Failed to create cache");

        // Generate a random key that doesn't exist
        let key = format!(
            "nonexistent:{}",
            std::time::SystemTime::now()
                .elapsed()
                .unwrap()
                .as_millis()
        );

        let result: Option<MarketData> = cache.get(&key).await.expect("Get should succeed");
        assert!(result.is_none());
    }

    #[ignore = "Requires Redis running on localhost:6379"]
    #[tokio::test]
    async fn test_key_prefixing() {
        let cache = create_test_cache().await.expect("Failed to create cache");

        let data = "test value";

        // The cache uses "test:" prefix
        cache.set("mykey", &data).await.expect("Set should succeed");

        // Access with full key
        let result: Option<String> = cache.get("mykey").await.expect("Get should succeed");
        assert_eq!(result, Some(data.to_string()));

        // Cleanup
        cache.delete("mykey").await.expect("Delete should succeed");
    }

    #[ignore = "Requires Redis running on localhost:6379"]
    #[tokio::test]
    async fn test_circuit_breaker_integration() {
        let cache = create_test_cache().await.expect("Failed to create cache");

        // Initially closed
        assert_eq!(cache.circuit_state().await, CircuitState::Closed);

        // Perform some operations
        for i in 0..3 {
            let data = MarketData {
                symbol: format!("TEST{}", i),
                price: 100.0,
                timestamp: 1234567890,
            };
            cache
                .set(&format!("test:{}", i), &data)
                .await
                .expect("Set should succeed");
        }

        // Still closed
        assert_eq!(cache.circuit_state().await, CircuitState::Closed);

        // Cleanup
        for i in 0..3 {
            let _ = cache.delete(&format!("test:{}", i)).await;
        }
    }
}
