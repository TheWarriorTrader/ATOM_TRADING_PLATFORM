//! # Risk Management Module
//!
//! Provides risk validation and monitoring for trading operations.
//!
//! This module contains the core risk engine that validates orders before
//! execution and monitors portfolio risk metrics.
//!
//! ## Components
//!
//! - [`RiskEngine`] - Main risk validation engine
//! - [`RiskConfig`] - Configuration for risk limits
//! - [`RiskState`] - Current risk engine state
//! - [`RiskDecision`] - Outcome of risk validation
//! - [`CircuitBreakerState`] - Circuit breaker state machine
//!
//! ## Usage
//!
//! ```rust
//! use application::risk::{RiskEngine, RiskConfig};
//! use domain::entities::{Order, Position};
//! use domain::values::Price;
//!
//! let config = RiskConfig::default();
//! let engine = RiskEngine::new(config);
//!
//! // Validate an order
//! let order = Order::new(/* ... */).unwrap();
//! let positions = vec![];
//! let current_price = Price::new(dec!(150)).unwrap();
//!
//! match engine.check_pre_trade(&order, &positions, current_price).unwrap() {
//!     RiskDecision::Allow => println!("Order approved"),
//!     RiskDecision::Reject { reason } => println!("Rejected: {}", reason),
//!     RiskDecision::Reduce { max_allowed } => println!("Reduce to {}", max_allowed),
//! }
//! ```

pub mod engine;
pub mod metrics;

pub use engine::{
    RiskEngine,
    RiskConfig,
    RiskState,
    RiskDecision,
    CircuitBreakerState,
};

pub use metrics::{
    RiskMetrics,
    RiskMetricsCalculator,
};