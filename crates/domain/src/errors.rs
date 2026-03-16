//! # Domain Errors
//!
//! Error types for domain operations with strict typing.
//!
//! This module provides a comprehensive hierarchy of error types for the domain layer,
//! designed to be type-safe, context-rich, serializable, and easily convertible.
//!
//! ## Error Hierarchy
//!
//! - [`DomainError`] - Top-level wrapper enum for all domain errors
//! - [`ValidationError`] - Input validation errors
//! - [`ProviderError`] - External provider errors (brokers, data feeds)
//! - [`RepositoryError`] - Persistence layer errors
//! - [`ExecutionError`] - Order execution errors
//! - [`StrategyError`] - Trading strategy errors
//!
//! ## Usage
//!
//! ```rust
//! use domain::errors::{DomainError, ValidationError, Result};
//!
//! fn validate_price(price: f64) -> Result<f64> {
//!     if price <= 0.0 {
//!         return Err(ValidationError::InvalidPrice {
//!             reason: "price must be positive".to_string(),
//!         }.into());
//!     }
//!     Ok(price)
//! }
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

/// Result type alias for domain operations.
///
/// This is a convenience type alias that uses [`DomainError`] as the error type.
/// Use this for all domain operations that can fail.
///
/// # Examples
///
/// ```rust
/// use domain::errors::Result;
///
/// fn do_something() -> Result<String> {
///     Ok("success".to_string())
/// }
/// ```
pub type Result<T> = std::result::Result<T, DomainError>;

/// Top-level error enum for the domain layer.
///
/// This enum acts as a "catch-all" wrapper for all specific error types in the domain layer.
/// It provides automatic conversion from specific error types via the [`From`] trait.
///
/// # Variants
///
/// - `Validation` - Input validation errors ([`ValidationError`])
/// - `Provider` - External provider errors ([`ProviderError`])
/// - `Repository` - Persistence errors ([`RepositoryError`])
/// - `Execution` - Order execution errors ([`ExecutionError`])
/// - `Strategy` - Strategy errors ([`StrategyError`])
/// - `Unknown` - Uncategorized errors with context
///
/// # Examples
///
/// ```rust
/// use domain::errors::{DomainError, ValidationError};
///
/// let validation_err = ValidationError::MissingField {
///     field: "price".to_string(),
/// };
/// let domain_err: DomainError = validation_err.into();
///
/// assert!(matches!(domain_err, DomainError::Validation(_)));
/// ```
#[derive(Error, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DomainError {
    /// Validation error wrapper.
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),

    /// Provider error wrapper.
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),

    /// Repository error wrapper.
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),

    /// Execution error wrapper.
    #[error("execution error: {0}")]
    Execution(#[from] ExecutionError),

    /// Strategy error wrapper.
    #[error("strategy error: {0}")]
    Strategy(#[from] StrategyError),

    /// Unknown/uncategorized error with context.
    #[error("unknown error: {message}")]
    Unknown {
        /// Error message
        message: String,
        /// Timestamp when the error occurred
        timestamp: DateTime<Utc>,
    },

    // =========================================================================
    // LEGACY VARIANTS - Maintained for backward compatibility
    // =========================================================================

    /// Invalid price value (legacy, use `Validation(ValidationError::InvalidPrice)`).
    #[error("Invalid price: {0}")]
    InvalidPrice(String),

    /// Invalid quantity value (legacy, use `Validation(ValidationError::InvalidQuantity)`).
    #[error("Invalid quantity: {0}")]
    InvalidQuantity(String),

    /// Order validation error (legacy, use `Validation(ValidationError::InvalidState)`).
    #[error("Order validation failed: {0}")]
    OrderValidation(String),

    /// Invalid state transition (legacy, use `Validation(ValidationError::InvalidState)`).
    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition {
        /// Current state
        from: String,
        /// Target state
        to: String,
    },

    /// Position not found (legacy, use `Repository(RepositoryError::NotFound)`).
    #[error("Position not found: {0}")]
    PositionNotFound(String),

    /// Symbol not found (legacy, use `Validation(ValidationError::InvalidSymbol)`).
    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    /// Not implemented (legacy, use `unknown` or `StrategyError::NotSupported`).
    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

impl DomainError {
    /// Creates an unknown error with the current timestamp.
    ///
    /// # Arguments
    ///
    /// * `message` - A description of what went wrong
    ///
    /// # Examples
    ///
    /// ```rust
    /// use domain::errors::DomainError;
    ///
    /// let err = DomainError::unknown("something unexpected happened");
    /// ```
    #[must_use]
    pub fn unknown(message: impl Into<String>) -> Self {
        Self::Unknown {
            message: message.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a validation error (legacy helper).
    ///
    /// # Arguments
    ///
    /// * `msg` - Validation error message
    ///
    /// # Examples
    ///
    /// ```rust
    /// use domain::errors::DomainError;
    ///
    /// let err = DomainError::validation("invalid input");
    /// ```
    #[must_use]
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(ValidationError::InvalidState {
            expected: "valid".to_string(),
            actual: msg.into(),
        })
    }

    /// Creates an invalid price error (legacy helper).
    ///
    /// # Arguments
    ///
    /// * `msg` - Error message
    #[must_use]
    pub fn invalid_price(msg: impl Into<String>) -> Self {
        Self::InvalidPrice(msg.into())
    }

    /// Creates an invalid quantity error (legacy helper).
    ///
    /// # Arguments
    ///
    /// * `msg` - Error message
    ///
    /// # Examples
    ///
    /// ```rust
    /// use domain::errors::DomainError;
    ///
    /// let err = DomainError::invalid_quantity("must be positive");
    /// ```
    #[must_use]
    pub fn invalid_quantity(msg: impl Into<String>) -> Self {
        Self::InvalidQuantity(msg.into())
    }
}

/// Legacy type alias for domain results.
///
/// Use [`Result`] for new code.
pub type DomainResult<T> = Result<T>;

/// Errors related to input validation.
///
/// These errors occur when input data doesn't respect business rules or constraints.
/// Each variant includes context to help identify and fix the validation issue.
///
/// # Examples
///
/// ```rust
/// use domain::errors::ValidationError;
///
/// let err = ValidationError::InvalidSymbol {
///     symbol: "INVALID!".to_string(),
///     reason: "contains special characters".to_string(),
/// };
///
/// assert_eq!(
///     err.to_string(),
///     "invalid symbol 'INVALID!': contains special characters"
/// );
/// ```
#[derive(Error, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ValidationError {
    /// Invalid price value.
    #[error("invalid price: {reason}")]
    InvalidPrice {
        /// Explanation of why the price is invalid
        reason: String,
    },

    /// Invalid quantity value.
    #[error("invalid quantity: {reason}")]
    InvalidQuantity {
        /// Explanation of why the quantity is invalid
        reason: String,
    },

    /// Invalid trading symbol.
    #[error("invalid symbol '{symbol}': {reason}")]
    InvalidSymbol {
        /// The invalid symbol
        symbol: String,
        /// Explanation of why the symbol is invalid
        reason: String,
    },

    /// Invalid time range.
    #[error("invalid time range: start {start} is after end {end}")]
    InvalidTimeRange {
        /// Start time
        start: DateTime<Utc>,
        /// End time
        end: DateTime<Utc>,
    },

    /// Invalid state transition or state value.
    #[error("invalid state: expected {expected}, got {actual}")]
    InvalidState {
        /// Expected state
        expected: String,
        /// Actual state received
        actual: String,
    },

    /// Missing required field.
    #[error("missing required field: {field}")]
    MissingField {
        /// Name of the missing field
        field: String,
    },

    /// Value out of allowed range.
    #[error("value out of range for {field}: expected {min}-{max}, got {actual}")]
    OutOfRange {
        /// Field name
        field: String,
        /// Minimum allowed value (as string for flexibility)
        min: String,
        /// Maximum allowed value (as string for flexibility)
        max: String,
        /// Actual value received (as string for flexibility)
        actual: String,
    },
}

/// Errors from external providers (brokers, data feeds, etc.).
///
/// These errors occur when interacting with external services such as
/// Interactive Brokers, Polygon.io, or other market data providers.
///
/// # Examples
///
/// ```rust
/// use domain::errors::ProviderError;
///
/// let err = ProviderError::ConnectionFailed {
///     provider: "InteractiveBrokers".to_string(),
///     reason: "timeout".to_string(),
/// };
/// ```
#[derive(Error, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProviderError {
    /// Failed to connect to provider.
    #[error("connection failed to {provider}: {reason}")]
    ConnectionFailed {
        /// Provider name
        provider: String,
        /// Reason for connection failure
        reason: String,
    },

    /// Disconnected from provider.
    #[error("disconnected from {provider}")]
    Disconnected {
        /// Provider name
        provider: String,
    },

    /// Operation timed out.
    #[error("timeout during {operation} after {duration_ms}ms")]
    Timeout {
        /// Operation that timed out
        operation: String,
        /// Duration in milliseconds
        duration_ms: u64,
    },

    /// Authentication failed.
    #[error("authentication failed for {provider}")]
    AuthenticationFailed {
        /// Provider name
        provider: String,
    },

    /// Rate limited by provider.
    #[error("rate limited by {provider}, retry after {retry_after_secs}s")]
    RateLimited {
        /// Provider name
        provider: String,
        /// Seconds to wait before retry
        retry_after_secs: u64,
    },

    /// Invalid response from provider.
    #[error("invalid response from {provider}: {details}")]
    InvalidResponse {
        /// Provider name
        provider: String,
        /// Details about the invalid response
        details: String,
    },

    /// Failed to subscribe to symbol data.
    #[error("subscription failed for {symbol} on {provider}: {reason}")]
    SubscriptionFailed {
        /// Symbol being subscribed
        symbol: String,
        /// Provider name
        provider: String,
        /// Reason for failure
        reason: String,
    },

    /// Feature not supported by provider.
    #[error("feature '{feature}' not supported by {provider}")]
    NotSupported {
        /// Feature name
        feature: String,
        /// Provider name
        provider: String,
    },
}

/// Errors related to data persistence.
///
/// These errors occur when interacting with databases, caches,
/// or other storage mechanisms.
///
/// # Examples
///
/// ```rust
/// use domain::errors::RepositoryError;
///
/// let err = RepositoryError::NotFound {
///     entity: "Order".to_string(),
///     id: "ord-123".to_string(),
/// };
/// ```
#[derive(Error, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RepositoryError {
    /// Failed to connect to storage.
    #[error("connection failed")]
    ConnectionFailed,

    /// Database query failed.
    #[error("query failed: {query} - {reason}")]
    QueryFailed {
        /// Query that failed
        query: String,
        /// Reason for failure
        reason: String,
    },

    /// Entity not found.
    #[error("{entity} not found: {id}")]
    NotFound {
        /// Entity type
        entity: String,
        /// Entity identifier
        id: String,
    },

    /// Duplicate key violation.
    #[error("duplicate key for {entity}: {key}")]
    DuplicateKey {
        /// Entity type
        entity: String,
        /// Duplicate key value
        key: String,
    },

    /// Constraint violation.
    #[error("constraint violation: {constraint} - {details}")]
    ConstraintViolation {
        /// Constraint name
        constraint: String,
        /// Details about the violation
        details: String,
    },

    /// Serialization/deserialization failed.
    #[error("serialization failed: {reason}")]
    SerializationFailed {
        /// Reason for serialization failure
        reason: String,
    },

    /// Transaction failed.
    #[error("transaction failed: {reason}")]
    TransactionFailed {
        /// Reason for transaction failure
        reason: String,
    },
}

impl RepositoryError {
    /// Creates a connection failed error.
    #[must_use]
    pub fn connection(_reason: impl Into<String>) -> Self {
        Self::ConnectionFailed
    }

    /// Creates a query failed error.
    #[must_use]
    pub fn query(reason: impl Into<String>) -> Self {
        Self::QueryFailed {
            query: "unknown".to_string(),
            reason: reason.into(),
        }
    }

    /// Creates a not found error.
    #[must_use]
    pub fn not_found(id: impl Into<String>) -> Self {
        Self::NotFound {
            entity: "Record".to_string(),
            id: id.into(),
        }
    }

    /// Creates a duplicate key error.
    #[must_use]
    pub fn duplicate_key(key: impl Into<String>) -> Self {
        Self::DuplicateKey {
            entity: "Record".to_string(),
            key: key.into(),
        }
    }

    /// Creates a constraint violation error.
    #[must_use]
    pub fn constraint_violation(details: impl Into<String>) -> Self {
        Self::ConstraintViolation {
            constraint: "unknown".to_string(),
            details: details.into(),
        }
    }

    /// Creates a serialization failed error.
    #[must_use]
    pub fn serialization(reason: impl Into<String>) -> Self {
        Self::SerializationFailed {
            reason: reason.into(),
        }
    }
}

/// Errors during order execution.
///
/// These errors occur when the broker rejects or fails to execute orders.
///
/// # Examples
///
/// ```rust
/// use domain::errors::ExecutionError;
///
/// let err = ExecutionError::OrderRejected {
///     order_id: "ord-123".to_string(),
///     reason: "insufficient funds".to_string(),
/// };
/// ```
#[derive(Error, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionError {
    /// Order was rejected by the broker.
    #[error("order {order_id} rejected: {reason}")]
    OrderRejected {
        /// Order identifier
        order_id: String,
        /// Reason for rejection
        reason: String,
    },

    /// Order not found.
    #[error("order not found: {order_id}")]
    OrderNotFound {
        /// Order identifier
        order_id: String,
    },

    /// Invalid order state for operation.
    #[error("invalid order state for {order_id}: {state} during {operation}")]
    InvalidOrderState {
        /// Order identifier
        order_id: String,
        /// Current order state
        state: String,
        /// Operation attempted
        operation: String,
    },

    /// Insufficient funds for order.
    #[error("insufficient funds: required {required}, available {available}")]
    InsufficientFunds {
        /// Required amount
        required: String,
        /// Available amount
        available: String,
    },

    /// Market is closed.
    #[error("market closed for {symbol}")]
    MarketClosed {
        /// Trading symbol
        symbol: String,
    },

    /// Price out of acceptable range.
    #[error("price {price} out of range [{min}, {max}]")]
    PriceOutOfRange {
        /// Submitted price
        price: String,
        /// Minimum allowed price
        min: String,
        /// Maximum allowed price
        max: String,
    },

    /// Quantity exceeds maximum allowed.
    #[error("quantity {quantity} exceeds maximum allowed {max_allowed}")]
    QuantityTooLarge {
        /// Submitted quantity
        quantity: String,
        /// Maximum allowed quantity
        max_allowed: String,
    },

    /// Exchange-specific error.
    #[error("exchange error from {exchange}: [{code}] {message}")]
    ExchangeError {
        /// Exchange name
        exchange: String,
        /// Error code
        code: String,
        /// Error message
        message: String,
    },
}

/// Errors from the strategy engine.
///
/// These errors occur during strategy initialization, configuration,
/// or execution.
///
/// # Examples
///
/// ```rust
/// use domain::errors::StrategyError;
///
/// let err = StrategyError::InvalidConfiguration {
///     strategy: "Momentum".to_string(),
///     field: "lookback_period".to_string(),
///     reason: "must be positive".to_string(),
/// };
/// ```
#[derive(Error, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StrategyError {
    /// Invalid strategy configuration.
    #[error("invalid configuration for {strategy}: {field} - {reason}")]
    InvalidConfiguration {
        /// Strategy name
        strategy: String,
        /// Configuration field
        field: String,
        /// Reason for invalidity
        reason: String,
    },

    /// Strategy initialization failed.
    #[error("initialization failed for {strategy}: {reason}")]
    InitializationFailed {
        /// Strategy name
        strategy: String,
        /// Reason for failure
        reason: String,
    },

    /// Strategy execution failed.
    #[error("execution failed for {strategy} on signal {signal}: {reason}")]
    ExecutionFailed {
        /// Strategy name
        strategy: String,
        /// Signal identifier
        signal: String,
        /// Reason for failure
        reason: String,
    },

    /// Invalid trading signal.
    #[error("invalid signal from {strategy}: {signal} - {reason}")]
    InvalidSignal {
        /// Strategy name
        strategy: String,
        /// Signal identifier
        signal: String,
        /// Reason for invalidity
        reason: String,
    },

    /// Risk violation detected.
    #[error("risk violation in {strategy}: {violation}")]
    RiskViolation {
        /// Strategy name
        strategy: String,
        /// Description of the violation
        violation: String,
    },
}

/// Contextual information for errors.
///
/// This struct provides additional context about when and where an error occurred,
/// useful for debugging and logging purposes.
///
/// # Examples
///
/// ```rust
/// use domain::errors::ErrorContext;
///
/// let ctx = ErrorContext::new("place_order")
///     .with_user_id("user-123");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorContext {
    /// Operation being performed when the error occurred
    pub operation: String,
    /// Timestamp when the error occurred
    pub timestamp: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Uuid,
    /// User ID if available
    pub user_id: Option<String>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl ErrorContext {
    /// Creates a new error context with the current timestamp and a new correlation ID.
    ///
    /// # Arguments
    ///
    /// * `operation` - The operation being performed
    ///
    /// # Examples
    ///
    /// ```rust
    /// use domain::errors::ErrorContext;
    ///
    /// let ctx = ErrorContext::new("place_order");
    /// ```
    #[must_use]
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            timestamp: Utc::now(),
            correlation_id: Uuid::new_v4(),
            user_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Sets the user ID.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The user identifier
    ///
    /// # Examples
    ///
    /// ```rust
    /// use domain::errors::ErrorContext;
    ///
    /// let ctx = ErrorContext::new("place_order")
    ///     .with_user_id("user-123");
    /// ```
    #[must_use]
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Adds metadata to the context.
    ///
    /// # Arguments
    ///
    /// * `key` - Metadata key
    /// * `value` - Metadata value
    ///
    /// # Examples
    ///
    /// ```rust
    /// use domain::errors::ErrorContext;
    ///
    /// let ctx = ErrorContext::new("place_order")
    ///     .with_metadata("symbol", "AAPL");
    /// ```
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// An error with associated context.
///
/// This struct wraps any error type with an [`ErrorContext`] for additional
/// debugging information.
///
/// # Type Parameters
///
/// * `E` - The error type being wrapped
///
/// # Examples
///
/// ```rust
/// use domain::errors::{ContextualError, ErrorContext, ValidationError};
///
/// let error = ValidationError::MissingField {
///     field: "price".to_string(),
/// };
///
/// let ctx = ErrorContext::new("validate_order");
/// let contextual = ContextualError::new(error, ctx);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextualError<E> {
    /// The underlying error
    pub error: E,
    /// Contextual information
    pub context: ErrorContext,
}

impl<E> ContextualError<E> {
    /// Creates a new contextual error.
    ///
    /// # Arguments
    ///
    /// * `error` - The error to wrap
    /// * `context` - The context for the error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use domain::errors::{ContextualError, ErrorContext, ValidationError};
    ///
    /// let error = ValidationError::MissingField {
    ///     field: "price".to_string(),
    /// };
    /// let ctx = ErrorContext::new("validate_order");
    ///
    /// let contextual = ContextualError::new(error, ctx);
    /// ```
    #[must_use]
    pub fn new(error: E, context: ErrorContext) -> Self {
        Self { error, context }
    }
}

impl<E: std::fmt::Display> std::fmt::Display for ContextualError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} (operation: {}, correlation_id: {})",
            self.error, self.context.operation, self.context.correlation_id
        )
    }
}

impl<E: std::error::Error> std::error::Error for ContextualError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.source()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // ValidationError Tests
    // =========================================================================

    #[test]
    fn validation_error_invalid_price_display() {
        let err = ValidationError::InvalidPrice {
            reason: "must be positive".to_string(),
        };
        assert_eq!(err.to_string(), "invalid price: must be positive");
    }

    #[test]
    fn validation_error_invalid_symbol_display() {
        let err = ValidationError::InvalidSymbol {
            symbol: "INVALID!".to_string(),
            reason: "contains special characters".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "invalid symbol 'INVALID!': contains special characters"
        );
    }

    #[test]
    fn validation_error_invalid_state_display() {
        let err = ValidationError::InvalidState {
            expected: "Open".to_string(),
            actual: "Closed".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "invalid state: expected Open, got Closed"
        );
    }

    #[test]
    fn validation_error_missing_field_display() {
        let err = ValidationError::MissingField {
            field: "price".to_string(),
        };
        assert_eq!(err.to_string(), "missing required field: price");
    }

    #[test]
    fn validation_error_out_of_range_display() {
        let err = ValidationError::OutOfRange {
            field: "quantity".to_string(),
            min: "1".to_string(),
            max: "1000".to_string(),
            actual: "1500".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "value out of range for quantity: expected 1-1000, got 1500"
        );
    }

    #[test]
    fn validation_error_invalid_time_range_display() {
        let start = Utc::now();
        let end = start - chrono::Duration::hours(1);
        let err = ValidationError::InvalidTimeRange { start, end };
        assert!(err.to_string().contains("invalid time range"));
    }

    // =========================================================================
    // ProviderError Tests
    // =========================================================================

    #[test]
    fn provider_error_connection_failed_display() {
        let err = ProviderError::ConnectionFailed {
            provider: "InteractiveBrokers".to_string(),
            reason: "timeout".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "connection failed to InteractiveBrokers: timeout"
        );
    }

    #[test]
    fn provider_error_rate_limited_display() {
        let err = ProviderError::RateLimited {
            provider: "Polygon".to_string(),
            retry_after_secs: 60,
        };
        assert_eq!(
            err.to_string(),
            "rate limited by Polygon, retry after 60s"
        );
    }

    #[test]
    fn provider_error_subscription_failed_display() {
        let err = ProviderError::SubscriptionFailed {
            symbol: "AAPL".to_string(),
            provider: "IB".to_string(),
            reason: "symbol not found".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "subscription failed for AAPL on IB: symbol not found"
        );
    }

    // =========================================================================
    // RepositoryError Tests
    // =========================================================================

    #[test]
    fn repository_error_not_found_display() {
        let err = RepositoryError::NotFound {
            entity: "Order".to_string(),
            id: "ord-123".to_string(),
        };
        assert_eq!(err.to_string(), "Order not found: ord-123");
    }

    #[test]
    fn repository_error_duplicate_key_display() {
        let err = RepositoryError::DuplicateKey {
            entity: "User".to_string(),
            key: "email@example.com".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "duplicate key for User: email@example.com"
        );
    }

    // =========================================================================
    // ExecutionError Tests
    // =========================================================================

    #[test]
    fn execution_error_order_rejected_display() {
        let err = ExecutionError::OrderRejected {
            order_id: "ord-123".to_string(),
            reason: "insufficient funds".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "order ord-123 rejected: insufficient funds"
        );
    }

    #[test]
    fn execution_error_insufficient_funds_display() {
        let err = ExecutionError::InsufficientFunds {
            required: "10000.00".to_string(),
            available: "5000.00".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "insufficient funds: required 10000.00, available 5000.00"
        );
    }

    #[test]
    fn execution_error_exchange_error_display() {
        let err = ExecutionError::ExchangeError {
            exchange: "NYSE".to_string(),
            code: "E123".to_string(),
            message: "Trading halted".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "exchange error from NYSE: [E123] Trading halted"
        );
    }

    // =========================================================================
    // StrategyError Tests
    // =========================================================================

    #[test]
    fn strategy_error_invalid_configuration_display() {
        let err = StrategyError::InvalidConfiguration {
            strategy: "Momentum".to_string(),
            field: "lookback_period".to_string(),
            reason: "must be positive".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "invalid configuration for Momentum: lookback_period - must be positive"
        );
    }

    #[test]
    fn strategy_error_risk_violation_display() {
        let err = StrategyError::RiskViolation {
            strategy: "Arbitrage".to_string(),
            violation: "position limit exceeded".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "risk violation in Arbitrage: position limit exceeded"
        );
    }

    // =========================================================================
    // DomainError (Wrapper) Tests
    // =========================================================================

    #[test]
    fn domain_error_from_validation() {
        let validation = ValidationError::MissingField {
            field: "price".to_string(),
        };
        let domain: DomainError = validation.into();

        assert!(matches!(domain, DomainError::Validation(_)));
        assert!(domain.to_string().contains("validation error"));
    }

    #[test]
    fn domain_error_from_provider() {
        let provider = ProviderError::Disconnected {
            provider: "IB".to_string(),
        };
        let domain: DomainError = provider.into();

        assert!(matches!(domain, DomainError::Provider(_)));
    }

    #[test]
    fn domain_error_from_repository() {
        let repo = RepositoryError::ConnectionFailed;
        let domain: DomainError = repo.into();

        assert!(matches!(domain, DomainError::Repository(_)));
    }

    #[test]
    fn domain_error_from_execution() {
        let exec = ExecutionError::MarketClosed {
            symbol: "AAPL".to_string(),
        };
        let domain: DomainError = exec.into();

        assert!(matches!(domain, DomainError::Execution(_)));
    }

    #[test]
    fn domain_error_from_strategy() {
        let strat = StrategyError::InitializationFailed {
            strategy: "Test".to_string(),
            reason: "config error".to_string(),
        };
        let domain: DomainError = strat.into();

        assert!(matches!(domain, DomainError::Strategy(_)));
    }

    #[test]
    fn domain_error_unknown() {
        let err = DomainError::unknown("unexpected failure");
        assert!(matches!(err, DomainError::Unknown { .. }));
        assert!(err.to_string().contains("unexpected failure"));
    }

    // =========================================================================
    // Serialization Tests
    // =========================================================================

    #[test]
    fn validation_error_serde_roundtrip() {
        let err = ValidationError::InvalidSymbol {
            symbol: "TEST".to_string(),
            reason: "test reason".to_string(),
        };

        let json = serde_json::to_string(&err).unwrap();
        let decoded: ValidationError = serde_json::from_str(&json).unwrap();

        assert_eq!(err, decoded);
    }

    #[test]
    fn provider_error_serde_roundtrip() {
        let err = ProviderError::Timeout {
            operation: "connect".to_string(),
            duration_ms: 5000,
        };

        let json = serde_json::to_string(&err).unwrap();
        let decoded: ProviderError = serde_json::from_str(&json).unwrap();

        assert_eq!(err, decoded);
    }

    #[test]
    fn execution_error_serde_roundtrip() {
        let err = ExecutionError::OrderRejected {
            order_id: "ord-123".to_string(),
            reason: "test".to_string(),
        };

        let json = serde_json::to_string(&err).unwrap();
        let decoded: ExecutionError = serde_json::from_str(&json).unwrap();

        assert_eq!(err.to_string(), decoded.to_string());
    }

    #[test]
    fn domain_error_serde_roundtrip() {
        let validation = ValidationError::MissingField {
            field: "test".to_string(),
        };
        let err = DomainError::Validation(validation);

        let json = serde_json::to_string(&err).unwrap();
        let decoded: DomainError = serde_json::from_str(&json).unwrap();

        assert_eq!(err, decoded);
    }

    // =========================================================================
    // ErrorContext Tests
    // =========================================================================

    #[test]
    fn error_context_new() {
        let ctx = ErrorContext::new("test_operation");

        assert_eq!(ctx.operation, "test_operation");
        assert!(ctx.user_id.is_none());
        assert!(ctx.metadata.is_empty());
    }

    #[test]
    fn error_context_with_user_id() {
        let ctx = ErrorContext::new("test").with_user_id("user-123");

        assert_eq!(ctx.user_id, Some("user-123".to_string()));
    }

    #[test]
    fn error_context_with_metadata() {
        let ctx = ErrorContext::new("test").with_metadata("key", "value");

        assert_eq!(ctx.metadata.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn error_context_builder_chain() {
        let ctx = ErrorContext::new("place_order")
            .with_user_id("user-123")
            .with_metadata("symbol", "AAPL")
            .with_metadata("quantity", "100");

        assert_eq!(ctx.operation, "place_order");
        assert_eq!(ctx.user_id, Some("user-123".to_string()));
        assert_eq!(ctx.metadata.get("symbol"), Some(&"AAPL".to_string()));
        assert_eq!(ctx.metadata.get("quantity"), Some(&"100".to_string()));
    }

    #[test]
    fn error_context_serde_roundtrip() {
        let ctx = ErrorContext::new("test")
            .with_user_id("user-123")
            .with_metadata("key", "value");

        let json = serde_json::to_string(&ctx).unwrap();
        let decoded: ErrorContext = serde_json::from_str(&json).unwrap();

        assert_eq!(ctx.operation, decoded.operation);
        assert_eq!(ctx.user_id, decoded.user_id);
        assert_eq!(ctx.metadata, decoded.metadata);
    }

    // =========================================================================
    // ContextualError Tests
    // =========================================================================

    #[test]
    fn contextual_error_new() {
        let error = ValidationError::MissingField {
            field: "price".to_string(),
        };
        let ctx = ErrorContext::new("validate_order");
        let contextual = ContextualError::new(error, ctx);

        assert_eq!(contextual.context.operation, "validate_order");
        assert!(contextual.to_string().contains("validate_order"));
    }

    #[test]
    fn contextual_error_display() {
        let error = ValidationError::InvalidPrice {
            reason: "negative".to_string(),
        };
        let ctx = ErrorContext::new("validate");
        let contextual = ContextualError::new(error, ctx);

        let display = contextual.to_string();
        assert!(display.contains("invalid price"));
        assert!(display.contains("validate"));
        assert!(display.contains("correlation_id"));
    }

    #[test]
    fn contextual_error_serde_roundtrip() {
        let error = ExecutionError::OrderNotFound {
            order_id: "ord-123".to_string(),
        };
        let ctx = ErrorContext::new("cancel_order").with_user_id("user-456");
        let contextual = ContextualError::new(error, ctx);

        let json = serde_json::to_string(&contextual).unwrap();
        let decoded: ContextualError<ExecutionError> = serde_json::from_str(&json).unwrap();

        assert_eq!(contextual.context.operation, decoded.context.operation);
        assert_eq!(contextual.context.user_id, decoded.context.user_id);
    }

    // =========================================================================
    // Result Type Alias Tests
    // =========================================================================

    #[test]
    fn result_type_alias_ok() {
        fn returns_ok() -> Result<String> {
            Ok("success".to_string())
        }

        assert!(returns_ok().is_ok());
    }

    #[test]
    fn result_type_alias_err() {
        fn returns_err() -> Result<String> {
            Err(ValidationError::MissingField {
                field: "test".to_string(),
            }
            .into())
        }

        let result = returns_err();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DomainError::Validation(_)));
    }
}
