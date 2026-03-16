//! # Provider Implementations
//!
//! This module contains implementations of domain provider traits
//! for market data, execution, and other external services.
//!
//! ## Modules
//!
//! - [`paper_trading`] - Paper trading execution gateway for simulation

/// Paper trading gateway implementation.
pub mod paper_trading;

// Re-export main types for convenience
pub use paper_trading::{PaperTradingConfig, PaperTradingGateway, PaperTradingPerformance};