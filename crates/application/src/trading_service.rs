//! # Trading Service
//!
//! Core orchestrator that coordinates the complete order flow from receipt to execution.
//!
//! This service implements the main trading workflow:
//! 1. Order validation
//! 2. Pre-trade risk checks
//! 3. Order persistence
//! 4. Order execution
//! 5. Event publishing
//!
//! ## Architecture
//!
//! Following hexagonal architecture principles:
//! - Domain layer (entities, values, events) is at the center
//! - Application layer (this service) coordinates use cases
//! - Infrastructure dependencies are injected via traits (ports)
//!
//! ## Error Handling
//!
//! All errors are converted to `TradingError` with actionable messages.
//! Rollback is performed on failure to maintain consistency.
//!
//! ## Thread Safety
//!
//! The service is thread-safe using `Arc` for shared dependencies.
//! All public methods are async and non-blocking.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tracing::{error, info, instrument, warn};

use domain::entities::{EntityId, Order, OrderStatus, Position};
use domain::errors::{DomainError, ExecutionError, RepositoryError, ValidationError};
use domain::events::TradingEvent;
use domain::execution::ExecutionGateway;
use domain::repositories::{OrderRepository, PositionRepository};
use domain::values::{OrderId, Price};

use crate::risk::{RiskDecision, RiskEngine};

// =============================================================================
// EVENT BUS PORT
// =============================================================================

/// Port for publishing trading events.
///
/// This trait abstracts the event bus implementation, allowing
/// the application layer to remain independent of infrastructure details.
#[async_trait]
pub trait EventBusPort: Send + Sync {
    /// Publishes a trading event to all subscribers.
    ///
    /// # Arguments
    ///
    /// * `event` - The trading event to publish
    ///
    /// # Returns
    ///
    /// Returns the number of receivers that received the event.
    async fn publish(&self, event: TradingEvent) -> usize;
}

// =============================================================================
// TRADING ERROR
// =============================================================================

/// Errors that can occur during trading operations.
///
/// This enum provides specific error variants for different failure modes
/// in the trading workflow, with actionable error messages.
#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum TradingError {
    /// Order validation failed.
    #[error("Order validation failed: {message}")]
    Validation {
        /// Human-readable error message
        message: String,
    },

    /// Order rejected by risk engine.
    #[error("Risk check rejected: {reason}")]
    RiskRejected {
        /// Reason for rejection
        reason: String,
    },

    /// Order execution failed.
    #[error("Execution failed: {message}")]
    Execution {
        /// Human-readable error message
        message: String,
    },

    /// Repository operation failed.
    #[error("Repository error: {message}")]
    Repository {
        /// Human-readable error message
        message: String,
    },

    /// Order not found.
    #[error("Order not found: {order_id}")]
    OrderNotFound {
        /// Order ID that was not found
        order_id: String,
    },

    /// Invalid order state for operation.
    #[error("Invalid order state: {message}")]
    InvalidState {
        /// Human-readable error message
        message: String,
    },

    /// Cancellation failed.
    #[error("Cancellation failed: {message}")]
    CancellationFailed {
        /// Human-readable error message
        message: String,
    },
}

impl TradingError {
    /// Creates a validation error.
    #[must_use]
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }

    /// Creates a risk rejected error.
    #[must_use]
    pub fn risk_rejected(reason: impl Into<String>) -> Self {
        Self::RiskRejected {
            reason: reason.into(),
        }
    }

    /// Creates an execution error.
    #[must_use]
    pub fn execution(message: impl Into<String>) -> Self {
        Self::Execution {
            message: message.into(),
        }
    }

    /// Creates a repository error.
    #[must_use]
    pub fn repository(message: impl Into<String>) -> Self {
        Self::Repository {
            message: message.into(),
        }
    }

    /// Creates an order not found error.
    #[must_use]
    pub fn order_not_found(order_id: impl Into<String>) -> Self {
        Self::OrderNotFound {
            order_id: order_id.into(),
        }
    }

    /// Creates an invalid state error.
    #[must_use]
    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self::InvalidState {
            message: message.into(),
        }
    }

    /// Creates a cancellation failed error.
    #[must_use]
    pub fn cancellation_failed(message: impl Into<String>) -> Self {
        Self::CancellationFailed {
            message: message.into(),
        }
    }

    /// Returns true if this is a risk rejection.
    #[must_use]
    pub fn is_risk_rejection(&self) -> bool {
        matches!(self, Self::RiskRejected { .. })
    }

    /// Returns true if this error is retryable.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Execution { .. } | Self::Repository { .. }
        )
    }
}

impl From<DomainError> for TradingError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::Validation(validation_err) => {
                Self::validation(validation_err.to_string())
            }
            DomainError::Repository(repo_err) => Self::repository(repo_err.to_string()),
            DomainError::Execution(exec_err) => Self::execution(exec_err.to_string()),
            DomainError::InvalidStateTransition { from, to } => {
                Self::invalid_state(format!("Cannot transition from {} to {}", from, to))
            }
            DomainError::OrderValidation(msg) => Self::validation(msg),
            DomainError::InvalidPrice(msg) => Self::validation(format!("Invalid price: {msg}")),
            DomainError::InvalidQuantity(msg) => {
                Self::validation(format!("Invalid quantity: {msg}"))
            }
            DomainError::PositionNotFound(id) => Self::order_not_found(id),
            DomainError::SymbolNotFound(sym) => Self::validation(format!("Symbol not found: {sym}")),
            _ => Self::execution(err.to_string()),
        }
    }
}

impl From<RepositoryError> for TradingError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::NotFound { entity, id } => {
                Self::order_not_found(format!("{entity} {id}"))
            }
            _ => Self::repository(err.to_string()),
        }
    }
}

impl From<ExecutionError> for TradingError {
    fn from(err: ExecutionError) -> Self {
        match err {
            ExecutionError::OrderRejected { order_id, reason } => {
                Self::execution(format!("Order {order_id} rejected: {reason}"))
            }
            ExecutionError::OrderNotFound { order_id } => Self::order_not_found(order_id),
            ExecutionError::InvalidOrderState {
                order_id,
                state,
                operation,
            } => Self::invalid_state(format!(
                "Order {order_id} in state {state} cannot perform {operation}"
            )),
            _ => Self::execution(err.to_string()),
        }
    }
}

impl From<ValidationError> for TradingError {
    fn from(err: ValidationError) -> Self {
        Self::validation(err.to_string())
    }
}

/// Result type alias for trading operations.
pub type TradingResult<T> = Result<T, TradingError>;

// =============================================================================
// TRADING SERVICE
// =============================================================================

/// Core trading service that orchestrates order flow.
///
/// The `TradingService` coordinates all aspects of order processing:
/// - Validation of order parameters
/// - Pre-trade risk checks via `RiskEngine`
/// - Persistence via `OrderRepository` and `PositionRepository`
/// - Execution via `ExecutionGateway`
/// - Event publishing via `EventBusPort`
///
/// ## Thread Safety
///
/// All dependencies are wrapped in `Arc` for thread-safe sharing.
/// The service can be safely cloned and used across multiple tasks.
///
/// ## Example
///
/// ```rust,ignore
/// use application::trading_service::TradingService;
/// use domain::entities::Order;
///
/// async fn place_order(service: &TradingService, order: Order) {
///     match service.place_order(order).await {
///         Ok(order_id) => println!("Order placed: {}", order_id),
///         Err(e) => eprintln!("Failed to place order: {}", e),
///     }
/// }
/// ```
#[derive(Clone)]
pub struct TradingService {
    /// Execution gateway for sending orders to market
    execution: Arc<dyn ExecutionGateway>,
    /// Repository for order persistence
    order_repo: Arc<dyn OrderRepository>,
    /// Repository for position persistence
    position_repo: Arc<dyn PositionRepository>,
    /// Risk engine for pre-trade validation
    risk_engine: Arc<RiskEngine>,
    /// Event bus for publishing trading events
    event_bus: Arc<dyn EventBusPort>,
}

impl std::fmt::Debug for TradingService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TradingService")
            .field("risk_engine", &self.risk_engine)
            .finish_non_exhaustive()
    }
}

impl TradingService {
    /// Creates a new TradingService with the given dependencies.
    ///
    /// # Arguments
    ///
    /// * `execution` - The execution gateway implementation
    /// * `order_repo` - The order repository implementation
    /// * `position_repo` - The position repository implementation
    /// * `risk_engine` - The risk engine for pre-trade checks
    /// * `event_bus` - The event bus for publishing events
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use application::trading_service::TradingService;
    /// use application::risk::RiskEngine;
    ///
    /// let service = TradingService::new(
    ///     execution_gateway,
    ///     order_repo,
    ///     position_repo,
    ///     risk_engine,
    ///     event_bus,
    /// );
    /// ```
    #[must_use]
    pub fn new(
        execution: Arc<dyn ExecutionGateway>,
        order_repo: Arc<dyn OrderRepository>,
        position_repo: Arc<dyn PositionRepository>,
        risk_engine: RiskEngine,
        event_bus: Arc<dyn EventBusPort>,
    ) -> Self {
        info!("TradingService initialized");
        Self {
            execution,
            order_repo,
            position_repo,
            risk_engine: Arc::new(risk_engine),
            event_bus,
        }
    }

    /// Places a new order through the complete workflow.
    ///
    /// The workflow consists of:
    /// 1. **Validation**: Validates order parameters using `order.validate_for_submission()`
    /// 2. **Risk Check**: Performs pre-trade risk validation via `risk_engine.check_pre_trade()`
    /// 3. **Persistence**: Saves order with "Submitted" status to repository
    /// 4. **Execution**: Sends order to market via `execution.place_order()`
    /// 5. **Event Publishing**: Publishes `OrderSubmitted` event
    ///
    /// If any step fails, rollback is performed to maintain consistency.
    ///
    /// # Arguments
    ///
    /// * `order` - The order to place
    ///
    /// # Returns
    ///
    /// Returns the assigned `OrderId` on success.
    ///
    /// # Errors
    ///
    /// Returns `TradingError::Validation` if order validation fails.
    /// Returns `TradingError::RiskRejected` if risk check fails.
    /// Returns `TradingError::Execution` if market execution fails.
    /// Returns `TradingError::Repository` if persistence fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use domain::entities::Order;
    /// use domain::values::{Symbol, Side, Quantity, OrderType};
    ///
    /// async fn example(service: &TradingService) -> Result<(), Box<dyn std::error::Error>> {
    ///     let order = Order::new(
    ///         account_id,
    ///         Symbol::new("AAPL")?,
    ///         Side::Buy,
    ///         OrderType::Market,
    ///         Quantity::new(100.into())?,
    ///         None,
    ///         None,
    ///     )?;
    ///
    ///     let order_id = service.place_order(order).await?;
    ///     println!("Order placed: {}", order_id);
    ///     Ok(())
    /// }
    /// ```
    #[instrument(skip(self, order), fields(order_id = %order.id(), symbol = %order.symbol()))]
    pub async fn place_order(&self, mut order: Order) -> TradingResult<OrderId> {
        info!("Starting order placement workflow");

        // Step 1: Validate order
        info!("Step 1: Validating order");
        if let Err(e) = order.validate_for_submission() {
            warn!("Order validation failed: {}", e);
            return Err(e.into());
        }

        // Step 2: Risk check pre-trade
        info!("Step 2: Performing pre-trade risk check");
        let positions = self.position_repo.find_open_positions().await.map_err(|e| {
            error!("Failed to fetch positions for risk check: {}", e);
            TradingError::from(e)
        })?;

        // Get current price for risk calculation (use limit price or default)
        let current_price = order.limit_price().unwrap_or_else(|| {
            // SAFETY: Default price is positive
            unsafe { Price::new_unchecked(rust_decimal::Decimal::ONE) }
        });

        match self.risk_engine.check_pre_trade(&order, &positions, current_price) {
            Ok(RiskDecision::Allow) => {
                info!("Risk check passed");
            }
            Ok(RiskDecision::Reject { reason }) => {
                warn!("Risk check rejected order: {}", reason);
                // Publish rejection event
                let event = TradingEvent::order_rejected(
                    OrderId::from_uuid(order.id()),
                    format!("Risk rejection: {}", reason),
                );
                self.event_bus.publish(event).await;
                return Err(TradingError::risk_rejected(reason));
            }
            Ok(RiskDecision::Reduce { max_allowed }) => {
                warn!("Risk check suggests reducing quantity to {}", max_allowed);
                // For now, reject if reduction is suggested
                // Future: could modify order quantity
                let reason = format!("Order quantity exceeds risk limits. Max allowed: {}", max_allowed);
                let event = TradingEvent::order_rejected(OrderId::from_uuid(order.id()), &reason);
                self.event_bus.publish(event).await;
                return Err(TradingError::risk_rejected(reason));
            }
            Err(e) => {
                error!("Risk engine error: {}", e);
                return Err(e.into());
            }
        }

        // Step 3: Persist order with "Submitted" status
        info!("Step 3: Persisting order");
        order.update_status(OrderStatus::Submitted { at: Utc::now() }).map_err(|e| {
            error!("Failed to update order status: {}", e);
            TradingError::from(e)
        })?;

        if let Err(e) = self.order_repo.save(&order).await {
            error!("Failed to save order to repository: {}", e);
            return Err(TradingError::from(e));
        }

        // Step 4: Execute order
        info!("Step 4: Executing order");
        let order_id = match self.execution.place_order(order.clone()).await {
            Ok(id) => {
                info!("Order executed successfully: {}", id);
                id
            }
            Err(e) => {
                error!("Order execution failed: {}", e);
                // Rollback: Update order status to Rejected
                if let Err(rollback_err) = self
                    .rollback_order_submission(order.id(), format!("Execution failed: {}", e))
                    .await
                {
                    error!("Rollback failed: {}", rollback_err);
                }
                return Err(TradingError::from(e));
            }
        };

        // Step 5: Publish event
        info!("Step 5: Publishing OrderSubmitted event");
        let event = TradingEvent::order_submitted(order_id, order);
        let receiver_count = self.event_bus.publish(event).await;
        info!("Event published to {} receivers", receiver_count);

        info!("Order placement workflow completed successfully");
        Ok(order_id)
    }

    /// Cancels an existing order.
    ///
    /// The cancellation workflow:
    /// 1. Retrieves the order from repository
    /// 2. Validates the order can be cancelled
    /// 3. Sends cancellation to execution gateway
    /// 4. Updates order status to Cancelled
    /// 5. Publishes `OrderCancelled` event
    ///
    /// # Arguments
    ///
    /// * `order_id` - The ID of the order to cancel
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful cancellation.
    ///
    /// # Errors
    ///
    /// Returns `TradingError::OrderNotFound` if the order doesn't exist.
    /// Returns `TradingError::InvalidState` if the order cannot be cancelled.
    /// Returns `TradingError::CancellationFailed` if the gateway rejects the cancellation.
    #[instrument(skip(self), fields(order_id = %order_id))]
    pub async fn cancel_order(&self, order_id: OrderId) -> TradingResult<()> {
        info!("Starting order cancellation workflow");

        // Step 1: Retrieve order
        let mut order = match self.order_repo.get(order_id).await {
            Ok(Some(order)) => order,
            Ok(None) => {
                warn!("Order not found for cancellation");
                return Err(TradingError::order_not_found(order_id.to_string()));
            }
            Err(e) => {
                error!("Failed to retrieve order: {}", e);
                return Err(TradingError::from(e));
            }
        };

        // Step 2: Validate order can be cancelled
        if !order.can_cancel() {
            let msg = format!("Order cannot be cancelled in state: {:?}", order.status());
            warn!("{}", msg);
            return Err(TradingError::invalid_state(msg));
        }

        // Step 3: Send cancellation to execution gateway
        info!("Sending cancellation to execution gateway");
        if let Err(e) = self.execution.cancel_order(order_id).await {
            error!("Execution gateway rejected cancellation: {}", e);
            return Err(TradingError::cancellation_failed(e.to_string()));
        }

        // Step 4: Update order status
        info!("Updating order status to Cancelled");
        if let Err(e) = order.cancel("User request".to_string()) {
            error!("Failed to update order status: {}", e);
            return Err(TradingError::from(e));
        }

        if let Err(e) = self.order_repo.update(&order).await {
            error!("Failed to update order in repository: {}", e);
            return Err(TradingError::from(e));
        }

        // Step 5: Publish event
        info!("Publishing OrderCancelled event");
        let event = TradingEvent::order_cancelled(order_id, "User request");
        self.event_bus.publish(event).await;

        info!("Order cancellation completed successfully");
        Ok(())
    }

    /// Retrieves an order by ID.
    ///
    /// # Arguments
    ///
    /// * `order_id` - The ID of the order to retrieve
    ///
    /// # Returns
    ///
    /// Returns `Some(Order)` if found, `None` if not found.
    ///
    /// # Errors
    ///
    /// Returns `TradingError::Repository` if the query fails.
    #[instrument(skip(self), fields(order_id = %order_id))]
    pub async fn get_order(&self, order_id: OrderId) -> TradingResult<Option<Order>> {
        match self.order_repo.get(order_id).await {
            Ok(order) => Ok(order),
            Err(e) => {
                error!("Failed to retrieve order: {}", e);
                Err(TradingError::from(e))
            }
        }
    }

    /// Retrieves all open (active) orders.
    ///
    /// Open orders are those that can still be filled:
    /// Created, Submitted, Pending, or PartiallyFilled.
    ///
    /// # Returns
    ///
    /// Returns a vector of all open orders.
    ///
    /// # Errors
    ///
    /// Returns `TradingError::Repository` if the query fails.
    #[instrument(skip(self))]
    pub async fn get_open_orders(&self) -> TradingResult<Vec<Order>> {
        match self.order_repo.get_open().await {
            Ok(orders) => {
                info!("Retrieved {} open orders", orders.len());
                Ok(orders)
            }
            Err(e) => {
                error!("Failed to retrieve open orders: {}", e);
                Err(TradingError::from(e))
            }
        }
    }

    /// Retrieves all current positions.
    ///
    /// # Returns
    ///
    /// Returns a vector of all open positions.
    ///
    /// # Errors
    ///
    /// Returns `TradingError::Repository` if the query fails.
    #[instrument(skip(self))]
    pub async fn get_positions(&self) -> TradingResult<Vec<Position>> {
        match self.position_repo.find_open_positions().await {
            Ok(positions) => {
                info!("Retrieved {} positions", positions.len());
                Ok(positions)
            }
            Err(e) => {
                error!("Failed to retrieve positions: {}", e);
                Err(TradingError::from(e))
            }
        }
    }

    /// Rolls back an order submission on failure.
    ///
    /// This is called when execution fails after the order has been
    /// persisted with "Submitted" status.
    ///
    /// # Arguments
    ///
    /// * `order_id` - The ID of the order to rollback
    /// * `reason` - The reason for the rollback
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful rollback.
    #[instrument(skip(self), fields(order_id = %order_id))]
    async fn rollback_order_submission(
        &self,
        order_id: EntityId,
        reason: String,
    ) -> TradingResult<()> {
        warn!("Rolling back order submission: {}", reason);

        // Try to get the order and update its status to Rejected
        if let Ok(Some(mut order)) = self.order_repo.get(OrderId::from_uuid(order_id)).await {
            if let Err(e) = order.reject(reason) {
                warn!("Failed to update order status during rollback: {}", e);
            } else if let Err(e) = self.order_repo.update(&order).await {
                warn!("Failed to save order during rollback: {}", e);
            } else {
                info!("Rollback completed successfully");
            }
        }

        Ok(())
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use domain::errors::RepositoryError;

    // =============================================================================
    // TRADING ERROR TESTS
    // =============================================================================

    #[test]
    fn test_trading_error_validation() {
        let err = TradingError::validation("invalid input");
        assert!(matches!(err, TradingError::Validation { ref message } if message == "invalid input"));
        assert_eq!(err.to_string(), "Order validation failed: invalid input");
    }

    #[test]
    fn test_trading_error_risk_rejected() {
        let err = TradingError::risk_rejected("exposure limit exceeded");
        assert!(matches!(err, TradingError::RiskRejected { ref reason } if reason == "exposure limit exceeded"));
        assert_eq!(err.to_string(), "Risk check rejected: exposure limit exceeded");
    }

    #[test]
    fn test_trading_error_execution() {
        let err = TradingError::execution("market closed");
        assert!(matches!(err, TradingError::Execution { ref message } if message == "market closed"));
        assert_eq!(err.to_string(), "Execution failed: market closed");
    }

    #[test]
    fn test_trading_error_repository() {
        let err = TradingError::repository("connection lost");
        assert!(matches!(err, TradingError::Repository { ref message } if message == "connection lost"));
        assert_eq!(err.to_string(), "Repository error: connection lost");
    }

    #[test]
    fn test_trading_error_order_not_found() {
        let err = TradingError::order_not_found("abc-123");
        assert!(matches!(err, TradingError::OrderNotFound { ref order_id } if order_id == "abc-123"));
        assert_eq!(err.to_string(), "Order not found: abc-123");
    }

    #[test]
    fn test_trading_error_invalid_state() {
        let err = TradingError::invalid_state("order already filled");
        assert!(matches!(err, TradingError::InvalidState { message } if message == "order already filled"));
    }

    #[test]
    fn test_trading_error_cancellation_failed() {
        let err = TradingError::cancellation_failed("gateway timeout");
        assert!(matches!(err, TradingError::CancellationFailed { ref message } if message == "gateway timeout"));
        assert_eq!(err.to_string(), "Cancellation failed: gateway timeout");
    }

    #[test]
    fn test_trading_error_is_risk_rejection() {
        let err = TradingError::risk_rejected("test");
        assert!(err.is_risk_rejection());

        let err = TradingError::validation("test");
        assert!(!err.is_risk_rejection());
    }

    #[test]
    fn test_trading_error_is_retryable() {
        // Execution and Repository errors are retryable
        assert!(TradingError::execution("test").is_retryable());
        assert!(TradingError::repository("test").is_retryable());

        // Validation and RiskRejected are not retryable
        assert!(!TradingError::validation("test").is_retryable());
        assert!(!TradingError::risk_rejected("test").is_retryable());
        assert!(!TradingError::order_not_found("test").is_retryable());
    }

    // =============================================================================
    // ERROR CONVERSION TESTS
    // =============================================================================

    #[test]
    fn test_domain_error_conversion_validation() {
        let domain_err = DomainError::validation("test error");
        let trading_err: TradingError = domain_err.into();
        assert!(matches!(trading_err, TradingError::Validation { .. }));
    }

    #[test]
    fn test_domain_error_conversion_repository() {
        let repo_err = RepositoryError::query("db error");
        let domain_err = DomainError::Repository(repo_err);
        let trading_err: TradingError = domain_err.into();
        assert!(matches!(trading_err, TradingError::Repository { .. }));
    }

    #[test]
    fn test_domain_error_conversion_execution() {
        let exec_err = ExecutionError::OrderRejected {
            order_id: "123".to_string(),
            reason: "insufficient funds".to_string(),
        };
        let domain_err = DomainError::Execution(exec_err);
        let trading_err: TradingError = domain_err.into();
        assert!(matches!(trading_err, TradingError::Execution { .. }));
    }

    #[test]
    fn test_domain_error_conversion_order_validation() {
        let domain_err = DomainError::OrderValidation("invalid quantity".to_string());
        let trading_err: TradingError = domain_err.into();
        assert!(matches!(trading_err, TradingError::Validation { .. }));
    }

    #[test]
    fn test_domain_error_conversion_invalid_price() {
        let domain_err = DomainError::InvalidPrice("negative value".to_string());
        let trading_err: TradingError = domain_err.into();
        assert!(matches!(trading_err, TradingError::Validation { .. }));
    }

    #[test]
    fn test_domain_error_conversion_invalid_quantity() {
        let domain_err = DomainError::InvalidQuantity("zero value".to_string());
        let trading_err: TradingError = domain_err.into();
        assert!(matches!(trading_err, TradingError::Validation { .. }));
    }

    #[test]
    fn test_domain_error_conversion_invalid_state_transition() {
        let domain_err = DomainError::InvalidStateTransition {
            from: "Filled".to_string(),
            to: "Submitted".to_string(),
        };
        let trading_err: TradingError = domain_err.into();
        assert!(matches!(trading_err, TradingError::InvalidState { .. }));
    }

    #[test]
    fn test_domain_error_conversion_position_not_found() {
        let domain_err = DomainError::PositionNotFound("pos-123".to_string());
        let trading_err: TradingError = domain_err.into();
        assert!(matches!(trading_err, TradingError::OrderNotFound { .. }));
    }

    #[test]
    fn test_repository_error_conversion_not_found() {
        let repo_err = RepositoryError::NotFound {
            entity: "Order".to_string(),
            id: "ord-123".to_string(),
        };
        let trading_err: TradingError = repo_err.into();
        assert!(matches!(trading_err, TradingError::OrderNotFound { order_id } if order_id == "Order ord-123"));
    }

    #[test]
    fn test_repository_error_conversion_query_failed() {
        let repo_err = RepositoryError::query("connection timeout");
        let trading_err: TradingError = repo_err.into();
        assert!(matches!(trading_err, TradingError::Repository { .. }));
    }

    #[test]
    fn test_execution_error_conversion_order_rejected() {
        let exec_err = ExecutionError::OrderRejected {
            order_id: "123".to_string(),
            reason: "market closed".to_string(),
        };
        let trading_err: TradingError = exec_err.into();
        assert!(matches!(trading_err, TradingError::Execution { message } if message.contains("market closed")));
    }

    #[test]
    fn test_execution_error_conversion_order_not_found() {
        let exec_err = ExecutionError::OrderNotFound {
            order_id: "abc-123".to_string(),
        };
        let trading_err: TradingError = exec_err.into();
        assert!(matches!(trading_err, TradingError::OrderNotFound { order_id } if order_id == "abc-123"));
    }

    #[test]
    fn test_execution_error_conversion_invalid_order_state() {
        let exec_err = ExecutionError::InvalidOrderState {
            order_id: "123".to_string(),
            state: "Filled".to_string(),
            operation: "Cancel".to_string(),
        };
        let trading_err: TradingError = exec_err.into();
        assert!(matches!(trading_err, TradingError::InvalidState { .. }));
    }

    #[test]
    fn test_validation_error_conversion() {
        let val_err = ValidationError::MissingField { field: "price".to_string() };
        let trading_err: TradingError = val_err.into();
        assert!(matches!(trading_err, TradingError::Validation { .. }));
    }

    // =============================================================================
    // EVENT BUS PORT TESTS
    // =============================================================================

    #[tokio::test]
    async fn test_event_bus_port_is_send_sync() {
        // This test verifies that EventBusPort is Send + Sync
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn EventBusPort>();
    }

    // =============================================================================
    // TRADING SERVICE STRUCT TESTS
    // =============================================================================

    #[test]
    fn test_trading_service_is_send_sync() {
        // Verify TradingService is Send + Sync for thread safety
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TradingService>();
    }

    #[test]
    fn test_trading_service_is_clone() {
        // Verify TradingService is Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<TradingService>();
    }

    #[test]
    fn test_trading_result_type() {
        // Verify TradingResult type works correctly
        let ok_result: TradingResult<i32> = Ok(42);
        assert!(ok_result.is_ok());

        let err_result: TradingResult<i32> = Err(TradingError::validation("test"));
        assert!(err_result.is_err());
    }
}
