//! # Infrastructure Crate
//!
//! Infrastructure layer containing external adapters and implementations.
//! This crate implements the interfaces defined in the domain and application layers.
//!
//! ## Architecture
//!
//! The infrastructure layer provides:
//! - Database repositories (SQLx implementations)
//! - External API clients (brokers, market data)
//! - Cache implementations (Redis)
//! - Message queue adapters
//! - Configuration management
//!
//! ## Dependencies
//!
//! - Depends on `domain` (for repository traits)
//! - Depends on `application` (for port implementations)
//! - Uses sqlx for database access
//! - Uses redis for caching
//! - Uses reqwest for HTTP clients

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

/// Database implementations (repositories, connection management)
pub mod database;

/// External API clients (broker APIs, market data providers)
pub mod external;

/// Provider implementations (paper trading, market data)
pub mod providers;

/// Cache implementations (Redis, in-memory)
pub mod cache;

/// Configuration management
pub mod config;

/// Error types for infrastructure operations
pub mod errors;

/// Logging and tracing utilities
pub mod logging;

// Re-export logging types for convenience
pub use logging::{
    CorrelationId, CorrelationIdVisitor, LogFormat, LogOutput, TracingConfig, clear_correlation_id,
    extract_correlation_id, get_correlation_id, init_tracing, init_tracing_for_integration_tests,
    init_tracing_for_tests, init_tracing_legacy, set_correlation_id, with_correlation_id,
};

/// Messaging infrastructure (event bus, pub/sub)
pub mod messaging;

/// Prometheus metrics collection and exposition
pub mod metrics;

/// Health checks and monitoring probes
pub mod health;

/// Unified monitoring server (metrics + health)
pub mod monitoring;

// Re-export commonly used types
pub use errors::{InfrastructureError, InfrastructureResult};

// Re-export metrics types
pub use metrics::{
    record_bars_ingested, record_bars_persisted, record_db_latency, record_error,
    set_active_subscribers, set_buffer_depth, set_circuit_breaker_state, set_provider_connected,
    update_processing_rate, MetricsCollector, MetricsServer, MetricsServerConfig,
};

// Re-export health types
pub use health::{
    ComponentHealth, ComponentStatus, HealthChecker, HealthServer, HealthServerConfig,
    OverallHealth, DatabaseHealthCheck, RedisHealthCheck, IBHealthCheck,
};

// Re-export monitoring types
pub use monitoring::{
    start_monitoring_server, MonitoringConfig, MonitoringServer, PipelineMetricsIntegration,
};
