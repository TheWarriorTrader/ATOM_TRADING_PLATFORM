//! # Application Crate
//!
//! Use cases and application orchestration layer.
//! This crate contains application-specific business logic and coordinates
//! between the domain layer and infrastructure layer.
//!
//! ## Architecture
//!
//! The application layer defines:
//! - Use Cases (CreateOrder, CancelOrder, etc.)
//! - DTOs (Data Transfer Objects)
//! - Application Services (orchestration)
//! - Ports (interfaces for infrastructure)
//!
//! ## Dependencies
//!
//! - Depends on `domain` crate (business logic)
//! - No dependencies on `infrastructure` (dependency inversion)
//! - Uses tokio for async runtime
//! - Uses tracing for observability

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

/// Use cases for order operations
pub mod order_use_cases;

/// Use cases for position operations
pub mod position_use_cases;

/// Use cases for account operations
pub mod account_use_cases;

/// Data Transfer Objects
pub mod dto;

/// Application services (orchestration)
pub mod services;

/// Error types for application layer
pub mod errors;

/// Port interfaces (driven by infrastructure)
pub mod ports;

// Re-export commonly used types
pub use dto::*;
pub use errors::{ApplicationError, ApplicationResult};
