//! # Cache Module
//!
//! Redis and in-memory cache implementations for the trading platform.
//!
//! ## Features
//!
//! - **Redis Cache**: High-performance distributed caching with:
//!   - Connection pooling via [`deadpool-redis`]
//!   - MessagePack serialization for efficiency
//!   - Circuit breaker pattern for resilience
//!   - Key prefixing for namespacing
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use infrastructure::cache::redis::RedisCache;
//! use infrastructure::config::RedisConfig;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = RedisConfig::default();
//! let cache = RedisCache::new(&config).await?;
//!
//! // Store data with TTL
//! cache.setex("bars:NQ:1m", &bars_data, 3600).await?;
//!
//! // Retrieve data
//! if let Some(bars) = cache.get::<_, Vec<Bar>>("bars:NQ:1m").await? {
//!     println!("Retrieved {} bars", bars.len());
//! }
//! # Ok(())
//! # }
//! ```

pub mod redis;

// Re-export commonly used types
pub use redis::{
    CircuitBreaker, CircuitBreakerConfig, CircuitState, RedisCache, RedisCacheConfig,
};
