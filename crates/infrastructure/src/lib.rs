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

/// Cache implementations (Redis, in-memory)
pub mod cache;

/// Configuration management
pub mod config;

/// Error types for infrastructure operations
pub mod errors;

/// Logging and tracing utilities
pub mod logging;

// Re-export commonly used types
pub use errors::{InfrastructureError, InfrastructureResult};
