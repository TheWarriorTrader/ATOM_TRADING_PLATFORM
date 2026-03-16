//! # Domain Crate
//!
//! Core business logic and domain entities for the trading platform.
//! This crate contains pure business logic with no external dependencies.
//!
//! ## Architecture
//!
//! The domain layer defines:
//! - Entities (Account, Order, Trade, Position)
//! - Value Objects (Price, Quantity, Money)
//! - Domain Events (OrderFilled, PositionOpened)
//! - Repository Traits (interfaces only)
//! - Domain Services (business rules)
//!
//! ## Dependencies
//!
//! No dependencies on other crates in the workspace.
//! External dependencies are limited to:
//! - `serde` for serialization
//! - `chrono` for datetime handling
//! - `uuid` for entity identifiers
//! - `rust_decimal` for financial calculations
//! - `thiserror` for error definitions

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

/// Entity definitions (Account, Order, Trade, Position)
pub mod entities;

/// Value objects (Price, Quantity, Money)
pub mod values;

/// Domain events (OrderFilled, PositionOpened)
pub mod events;

/// Repository trait definitions (interfaces only)
pub mod repositories;

/// Provider trait definitions (market data, execution)
pub mod providers;

/// Domain services (business rules)
pub mod services;

/// Error types for domain operations
pub mod errors;

/// Execution gateway trait for order routing
pub mod execution;

// Re-export commonly used types
#[allow(ambiguous_glob_reexports)]
pub use entities::*;
pub use errors::{
    ContextualError, DomainError, DomainResult, ErrorContext, ExecutionError, ProviderError,
    RepositoryError, Result, StrategyError, ValidationError,
};
pub use events::*;
pub use providers::*;
pub use repositories::*;
pub use services::*;
pub use execution::*;
#[allow(ambiguous_glob_reexports)]
pub use values::*;
