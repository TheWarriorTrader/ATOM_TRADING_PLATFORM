//! # Execution Gateway Trait
//!
//! Abstract interface for order execution gateways.
//!
//! This module defines the [`ExecutionGateway`] trait, which provides a unified interface
//! for executing orders across different brokers and trading environments (Interactive Brokers,
//! paper trading, simulation, etc.).
//!
//! ## Architecture
//!
//! Following hexagonal architecture principles:
//! - The **domain** layer defines the port ([`ExecutionGateway`] trait)
//! - The **infrastructure** layer provides adapters (IBExecutionGateway, PaperTradingGateway, etc.)
//!
//! ## Key Types
//!
//! - [`ExecutionGateway`] - The main trait for order execution
//! - [`OrderModifications`] - Builder pattern for order modifications
//! - [`OrderUpdate`] - Real-time order status updates via streaming
//!
//! ## Usage
//!
//! ```rust,ignore
//! use domain::execution::ExecutionGateway;
//! use domain::entities::Order;
//! use domain::values::OrderId;
//!
//! async fn place_order<G: ExecutionGateway>(
//!     gateway: &G,
//!     order: Order,
//! ) -> Result<OrderId, ExecutionError> {
//!     gateway.place_order(order).await
//! }
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc;

use crate::entities::{Fill, Order, OrderStatus, Position};
use crate::errors::ExecutionError;
use crate::values::{Money, OrderId, Price, Quantity, TimeInForce};

/// Trait for order execution gateways.
///
/// This trait defines the contract for all execution gateways, including:
/// - Interactive Brokers ([`IBExecutionGateway`](crate::external::ib::IBExecutionGateway))
/// - Paper trading ([`PaperTradingGateway`])
/// - Simulation/backtesting ([`SimulationGateway`])
///
/// # Type Safety
///
/// All methods use strong typing:
/// - [`OrderId`] for order identifiers (never raw strings)
/// - [`Quantity`] for order quantities (never raw f64)
/// - [`Price`] for prices (never raw f64)
///
/// # Concurrency
///
/// - The gateway must be thread-safe (`Send + Sync`)
/// - All methods are async for non-blocking I/O
/// - Streaming methods return [`mpsc::Receiver`] for backpressure handling
///
/// # Error Handling
///
/// All methods return [`Result<T, ExecutionError>`] with specific error variants:
/// - [`ExecutionError::OrderRejected`] - Order rejected by broker
/// - [`ExecutionError::OrderNotFound`] - Order ID not found
/// - [`ExecutionError::InvalidOrderState`] - Operation not valid for current state
/// - [`ExecutionError::InsufficientFunds`] - Not enough buying power
/// - [`ExecutionError::MarketClosed`] - Market not open for trading
/// - [`ExecutionError::ExchangeError`] - Generic exchange error
///
/// # Examples
///
/// ```rust,ignore
/// use domain::execution::ExecutionGateway;
/// use domain::entities::Order;
/// use domain::values::{Symbol, Side, Quantity, Price, OrderType};
/// use rust_decimal::Decimal;
///
/// async fn execute_market_order<G: ExecutionGateway>(
///     gateway: &G,
/// ) -> Result<(), Box<dyn std::error::Error>> {
///     // Create a market order
///     let order = Order::new(
///         account_id,
///         Symbol::new("AAPL")?,
///         Side::Buy,
///         OrderType::Market,
///         Quantity::new(Decimal::new(100, 0))?,
///         None, // No limit price for market orders
///         None, // No stop price for market orders
///     )?;
///
///     // Place the order
///     let order_id = gateway.place_order(order).await?;
///     println!("Order placed: {}", order_id);
///
///     // Subscribe to fill updates
///     let mut fill_rx = gateway.subscribe_fill_updates().await?;
///     while let Some(fill) = fill_rx.recv().await {
///         println!("Fill received: {:?}", fill);
///     }
///
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait ExecutionGateway: Send + Sync {
    /// Submits a new order to the broker/exchange.
    ///
    /// # Arguments
    ///
    /// * `order` - The order to place (see [`Order`] for details)
    ///
    /// # Returns
    ///
    /// Returns the assigned [`OrderId`] on success.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError::OrderRejected`] if the broker rejects the order.
    /// Returns [`ExecutionError::InsufficientFunds`] if buying power is insufficient.
    /// Returns [`ExecutionError::MarketClosed`] if the market is not open.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let order = Order::new(account_id, symbol, side, order_type, qty, None, None)?;
    /// let order_id = gateway.place_order(order).await?;
    /// ```
    async fn place_order(&self, order: Order) -> Result<OrderId, ExecutionError>;

    /// Cancels an existing order.
    ///
    /// # Arguments
    ///
    /// * `order_id` - The ID of the order to cancel
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError::OrderNotFound`] if the order doesn't exist.
    /// Returns [`ExecutionError::InvalidOrderState`] if the order is already filled or cancelled.
    ///
    /// # Note
    ///
    /// Cancellation is not guaranteed. The order may fill before the cancel
    /// request reaches the exchange. Always check order status via
    /// [`subscribe_order_updates`](Self::subscribe_order_updates).
    async fn cancel_order(&self, order_id: OrderId) -> Result<(), ExecutionError>;

    /// Modifies an existing order.
    ///
    /// This method allows changing order parameters (quantity, limit price,
    /// stop price, time in force) without cancelling and replacing the order.
    ///
    /// # Arguments
    ///
    /// * `order_id` - The ID of the order to modify
    /// * `modifications` - The changes to apply (see [`OrderModifications`])
    ///
    /// # When to Use Modify vs Cancel+Replace
    ///
    /// Use [`modify_order`](Self::modify_order) when:
    /// - You want to preserve the order's place in the queue
    /// - Only price/quantity needs to change
    /// - The exchange supports native modification
    ///
    /// Use cancel+place when:
    /// - Changing fundamental order properties (symbol, side)
    /// - The order type needs to change
    /// - You want a new order ID for tracking
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError::OrderNotFound`] if the order doesn't exist.
    /// Returns [`ExecutionError::InvalidOrderState`] if the order cannot be modified.
    async fn modify_order(
        &self,
        order_id: OrderId,
        modifications: OrderModifications,
    ) -> Result<(), ExecutionError>;

    /// Retrieves the current state of a specific order.
    ///
    /// # Arguments
    ///
    /// * `order_id` - The ID of the order to retrieve
    ///
    /// # Returns
    ///
    /// Returns `Some(Order)` if found, `None` if not found.
    ///
    /// # Note
    ///
    /// This returns a snapshot. For real-time updates, use
    /// [`subscribe_order_updates`](Self::subscribe_order_updates).
    async fn get_order(&self, order_id: OrderId) -> Result<Option<Order>, ExecutionError>;

    /// Retrieves all open (active) orders.
    ///
    /// # Returns
    ///
    /// Returns a vector of all orders that are not in a terminal state
    /// (not filled, cancelled, or rejected).
    ///
    /// # Note
    ///
    /// This returns a snapshot. Orders may change state immediately after
    /// this call returns.
    async fn get_open_orders(&self) -> Result<Vec<Order>, ExecutionError>;

    /// Retrieves all current positions.
    ///
    /// # Returns
    ///
    /// Returns a vector of all open positions across all symbols.
    ///
    /// # Note
    ///
    /// Positions include both long and short positions. A zero position
    /// may be included if the position was just closed.
    async fn get_positions(&self) -> Result<Vec<Position>, ExecutionError>;

    /// Subscribes to real-time order status updates.
    ///
    /// # Returns
    ///
    /// Returns a [`mpsc::Receiver`] that receives [`OrderUpdate`] events
    /// for all orders managed by this gateway.
    ///
    /// # Semantics
    ///
    /// - Updates are sent for ALL order state changes
    /// - The receiver should be processed continuously to avoid backpressure
    /// - Updates may arrive out of order during high activity
    /// - The channel has a bounded buffer; slow consumers may miss updates
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError::ConnectionFailed`] if the subscription cannot be established.
    async fn subscribe_order_updates(&self) -> Result<mpsc::Receiver<OrderUpdate>, ExecutionError>;

    /// Subscribes to real-time fill (execution) updates.
    ///
    /// # Returns
    ///
    /// Returns a [`mpsc::Receiver`] that receives [`Fill`] events
    /// for all executions.
    ///
    /// # Semantics
    ///
    /// - One order may generate multiple fills (partial fills)
    /// - Fill events include commission information if available
    /// - The receiver should be processed continuously
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError::ConnectionFailed`] if the subscription cannot be established.
    async fn subscribe_fill_updates(&self) -> Result<mpsc::Receiver<Fill>, ExecutionError>;
}

/// Modifications to apply to an existing order.
///
/// Uses the builder pattern for ergonomic construction:
///
/// ```rust
/// use domain::execution::OrderModifications;
/// use domain::values::{Quantity, Price};
/// use rust_decimal::Decimal;
///
/// let qty = Quantity::new(Decimal::new(200, 0)).unwrap();
/// let price = Price::new(Decimal::new(15050, 2)).unwrap();
///
/// let mods = OrderModifications::new()
///     .with_quantity(qty)
///     .with_limit_price(price);
/// ```
///
/// All fields are optional. Only specified fields will be modified.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrderModifications {
    /// New order quantity (if changing)
    pub quantity: Option<Quantity>,
    /// New limit price (if changing)
    pub limit_price: Option<Price>,
    /// New stop price (if changing)
    pub stop_price: Option<Price>,
    /// New time in force (if changing)
    pub time_in_force: Option<TimeInForce>,
}

impl OrderModifications {
    /// Creates a new empty modification set.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::execution::OrderModifications;
    ///
    /// let mods = OrderModifications::new();
    /// assert!(mods.quantity.is_none());
    /// assert!(mods.limit_price.is_none());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the quantity modification.
    ///
    /// # Arguments
    ///
    /// * `qty` - The new quantity
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::execution::OrderModifications;
    /// use domain::values::Quantity;
    /// use rust_decimal::Decimal;
    ///
    /// let qty = Quantity::new(Decimal::new(100, 0)).unwrap();
    /// let mods = OrderModifications::new().with_quantity(qty);
    /// assert!(mods.quantity.is_some());
    /// ```
    #[must_use]
    pub fn with_quantity(mut self, qty: Quantity) -> Self {
        self.quantity = Some(qty);
        self
    }

    /// Sets the limit price modification.
    ///
    /// # Arguments
    ///
    /// * `price` - The new limit price
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::execution::OrderModifications;
    /// use domain::values::Price;
    /// use rust_decimal::Decimal;
    ///
    /// let price = Price::new(Decimal::new(15050, 2)).unwrap();
    /// let mods = OrderModifications::new().with_limit_price(price);
    /// assert!(mods.limit_price.is_some());
    /// ```
    #[must_use]
    pub fn with_limit_price(mut self, price: Price) -> Self {
        self.limit_price = Some(price);
        self
    }

    /// Sets the stop price modification.
    ///
    /// # Arguments
    ///
    /// * `price` - The new stop price
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::execution::OrderModifications;
    /// use domain::values::Price;
    /// use rust_decimal::Decimal;
    ///
    /// let price = Price::new(Decimal::new(14500, 2)).unwrap();
    /// let mods = OrderModifications::new().with_stop_price(price);
    /// assert!(mods.stop_price.is_some());
    /// ```
    #[must_use]
    pub fn with_stop_price(mut self, price: Price) -> Self {
        self.stop_price = Some(price);
        self
    }

    /// Sets the time in force modification.
    ///
    /// # Arguments
    ///
    /// * `tif` - The new time in force
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::execution::OrderModifications;
    /// use domain::values::TimeInForce;
    ///
    /// let mods = OrderModifications::new().with_time_in_force(TimeInForce::GTC);
    /// assert!(mods.time_in_force.is_some());
    /// ```
    #[must_use]
    pub fn with_time_in_force(mut self, tif: TimeInForce) -> Self {
        self.time_in_force = Some(tif);
        self
    }

    /// Returns true if no modifications are specified.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.quantity.is_none()
            && self.limit_price.is_none()
            && self.stop_price.is_none()
            && self.time_in_force.is_none()
    }
}

/// Real-time order status update.
///
/// This struct represents a snapshot of an order's state at a specific point in time.
/// It is sent through the channel returned by [`ExecutionGateway::subscribe_order_updates`].
///
/// # Fields
///
/// - `order_id` - The order identifier
/// - `timestamp` - When this update was generated
/// - `status` - Current order status
/// - `filled_quantity` - Total quantity filled so far
/// - `remaining_quantity` - Quantity still outstanding
/// - `avg_fill_price` - Average price of all fills (if any)
/// - `last_fill_price` - Price of the most recent fill
/// - `last_fill_quantity` - Quantity of the most recent fill
/// - `commission` - Total commission paid (if available)
/// - `reason` - Additional context (e.g., rejection reason)
#[derive(Debug, Clone, PartialEq)]
pub struct OrderUpdate {
    /// The order identifier
    pub order_id: OrderId,
    /// When this update was generated
    pub timestamp: DateTime<Utc>,
    /// Current order status
    pub status: OrderStatus,
    /// Total quantity filled so far
    pub filled_quantity: Quantity,
    /// Quantity still outstanding
    pub remaining_quantity: Quantity,
    /// Average price of all fills (None if no fills yet)
    pub avg_fill_price: Option<Price>,
    /// Price of the most recent fill (None if no fills yet)
    pub last_fill_price: Option<Price>,
    /// Quantity of the most recent fill (None if no fills yet)
    pub last_fill_quantity: Option<Quantity>,
    /// Total commission paid (if available)
    pub commission: Option<Money>,
    /// Additional context (e.g., rejection/cancellation reason)
    pub reason: Option<String>,
}

impl OrderUpdate {
    /// Creates a new order update with the required fields.
    ///
    /// # Arguments
    ///
    /// * `order_id` - The order identifier
    /// * `status` - Current order status
    /// * `filled_quantity` - Total filled quantity
    /// * `remaining_quantity` - Remaining quantity
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use domain::execution::OrderUpdate;
    /// use domain::entities::OrderStatus;
    /// use domain::values::{OrderId, Quantity};
    /// use rust_decimal::Decimal;
    ///
    /// let update = OrderUpdate::new(
    ///     OrderId::generate(),
    ///     OrderStatus::Submitted { at: Utc::now() },
    ///     unsafe { Quantity::new_unchecked(Decimal::ZERO) },
    ///     Quantity::new(Decimal::new(100, 0)).unwrap(),
    /// );
    /// ```
    #[must_use]
    pub fn new(
        order_id: OrderId,
        status: OrderStatus,
        filled_quantity: Quantity,
        remaining_quantity: Quantity,
    ) -> Self {
        Self {
            order_id,
            timestamp: Utc::now(),
            status,
            filled_quantity,
            remaining_quantity,
            avg_fill_price: None,
            last_fill_price: None,
            last_fill_quantity: None,
            commission: None,
            reason: None,
        }
    }

    /// Sets the average fill price.
    #[must_use]
    pub fn with_avg_fill_price(mut self, price: Price) -> Self {
        self.avg_fill_price = Some(price);
        self
    }

    /// Sets the last fill details.
    #[must_use]
    pub fn with_last_fill(mut self, price: Price, quantity: Quantity) -> Self {
        self.last_fill_price = Some(price);
        self.last_fill_quantity = Some(quantity);
        self
    }

    /// Sets the commission.
    #[must_use]
    pub fn with_commission(mut self, commission: Money) -> Self {
        self.commission = Some(commission);
        self
    }

    /// Sets the reason string.
    #[must_use]
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Returns true if the order is completely filled.
    #[must_use]
    pub fn is_filled(&self) -> bool {
        matches!(self.status, OrderStatus::Filled { .. })
    }

    /// Returns true if the order is in a terminal state.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{OrderStatus, OrderType};
    use crate::values::Side;
    use crate::values::{Currency, Symbol};
    use rust_decimal::Decimal;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    // =================================================================================
    // OrderModifications Tests
    // =================================================================================

    #[test]
    fn order_modifications_new_is_empty() {
        let mods = OrderModifications::new();
        assert!(mods.quantity.is_none());
        assert!(mods.limit_price.is_none());
        assert!(mods.stop_price.is_none());
        assert!(mods.time_in_force.is_none());
        assert!(mods.is_empty());
    }

    #[test]
    fn order_modifications_default_is_empty() {
        let mods = OrderModifications::default();
        assert!(mods.is_empty());
    }

    #[test]
    fn order_modifications_with_quantity() {
        let qty = Quantity::new(Decimal::new(100, 0)).unwrap();
        let mods = OrderModifications::new().with_quantity(qty);

        assert_eq!(mods.quantity, Some(qty));
        assert!(mods.limit_price.is_none());
        assert!(!mods.is_empty());
    }

    #[test]
    fn order_modifications_with_limit_price() {
        let price = Price::new(Decimal::new(15050, 2)).unwrap();
        let mods = OrderModifications::new().with_limit_price(price);

        assert_eq!(mods.limit_price, Some(price));
        assert!(mods.quantity.is_none());
    }

    #[test]
    fn order_modifications_with_stop_price() {
        let price = Price::new(Decimal::new(14500, 2)).unwrap();
        let mods = OrderModifications::new().with_stop_price(price);

        assert_eq!(mods.stop_price, Some(price));
    }

    #[test]
    fn order_modifications_with_time_in_force() {
        let mods = OrderModifications::new().with_time_in_force(TimeInForce::GTC);

        assert_eq!(mods.time_in_force, Some(TimeInForce::GTC));
    }

    #[test]
    fn order_modifications_chaining() {
        let qty = Quantity::new(Decimal::new(200, 0)).unwrap();
        let price = Price::new(Decimal::new(15050, 2)).unwrap();

        let mods = OrderModifications::new()
            .with_quantity(qty)
            .with_limit_price(price)
            .with_time_in_force(TimeInForce::Day);

        assert_eq!(mods.quantity, Some(qty));
        assert_eq!(mods.limit_price, Some(price));
        assert_eq!(mods.time_in_force, Some(TimeInForce::Day));
        assert!(mods.stop_price.is_none());
    }

    #[test]
    fn order_modifications_clone() {
        let qty = Quantity::new(Decimal::new(100, 0)).unwrap();
        let mods = OrderModifications::new().with_quantity(qty);
        let cloned = mods.clone();

        assert_eq!(mods, cloned);
    }

    // =================================================================================
    // OrderUpdate Tests
    // =================================================================================

    #[test]
    fn order_update_new() {
        let order_id = OrderId::generate();
        let filled_qty = unsafe { Quantity::new_unchecked(Decimal::ZERO) };
        let remaining_qty = Quantity::new(Decimal::new(100, 0)).unwrap();
        let status = OrderStatus::Submitted { at: Utc::now() };

        let update = OrderUpdate::new(order_id, status.clone(), filled_qty, remaining_qty);

        assert_eq!(update.order_id, order_id);
        assert_eq!(update.status, status);
        assert_eq!(update.filled_quantity, filled_qty);
        assert_eq!(update.remaining_quantity, remaining_qty);
        assert!(update.avg_fill_price.is_none());
        assert!(update.last_fill_price.is_none());
        assert!(update.commission.is_none());
        assert!(update.reason.is_none());
    }

    #[test]
    fn order_update_with_avg_fill_price() {
        let order_id = OrderId::generate();
        let filled_qty = Quantity::new(Decimal::new(50, 0)).unwrap();
        let remaining_qty = Quantity::new(Decimal::new(50, 0)).unwrap();
        let status = OrderStatus::PartiallyFilled {
            filled: Quantity::new(Decimal::new(50, 0)).unwrap(),
            remaining: Quantity::new(Decimal::new(50, 0)).unwrap(),
            avg_price: Price::new(Decimal::new(15000, 2)).unwrap(),
        };

        let update = OrderUpdate::new(order_id, status, filled_qty, remaining_qty)
            .with_avg_fill_price(Price::new(Decimal::new(15025, 2)).unwrap());

        assert!(update.avg_fill_price.is_some());
    }

    #[test]
    fn order_update_with_last_fill() {
        let order_id = OrderId::generate();
        let filled_qty = Quantity::new(Decimal::new(50, 0)).unwrap();
        let remaining_qty = Quantity::new(Decimal::new(50, 0)).unwrap();
        let status = OrderStatus::PartiallyFilled {
            filled: Quantity::new(Decimal::new(50, 0)).unwrap(),
            remaining: Quantity::new(Decimal::new(50, 0)).unwrap(),
            avg_price: Price::new(Decimal::new(15000, 2)).unwrap(),
        };
        let last_price = Price::new(Decimal::new(15050, 2)).unwrap();
        let last_qty = Quantity::new(Decimal::new(25, 0)).unwrap();

        let update = OrderUpdate::new(order_id, status, filled_qty, remaining_qty)
            .with_last_fill(last_price, last_qty);

        assert_eq!(update.last_fill_price, Some(last_price));
        assert_eq!(update.last_fill_quantity, Some(last_qty));
    }

    #[test]
    fn order_update_with_commission() {
        let order_id = OrderId::generate();
        let filled_qty = Quantity::new(Decimal::new(100, 0)).unwrap();
        let remaining_qty = unsafe { Quantity::new_unchecked(Decimal::ZERO) };
        let status = OrderStatus::Filled { at: Utc::now() };
        let commission = Money::new(Decimal::new(125, 2), Currency::USD).unwrap();

        let update = OrderUpdate::new(order_id, status, filled_qty, remaining_qty)
            .with_commission(commission);

        assert_eq!(update.commission, Some(commission));
    }

    #[test]
    fn order_update_with_reason() {
        let order_id = OrderId::generate();
        let filled_qty = unsafe { Quantity::new_unchecked(Decimal::ZERO) };
        let remaining_qty = Quantity::new(Decimal::new(100, 0)).unwrap();
        let status = OrderStatus::Rejected {
            at: Utc::now(),
            reason: "Insufficient funds".to_string(),
        };

        let update = OrderUpdate::new(order_id, status, filled_qty, remaining_qty)
            .with_reason("Order rejected by risk management");

        assert_eq!(update.reason, Some("Order rejected by risk management".to_string()));
    }

    #[test]
    fn order_update_is_filled() {
        let order_id = OrderId::generate();
        let filled_qty = Quantity::new(Decimal::new(100, 0)).unwrap();
        let remaining_qty = unsafe { Quantity::new_unchecked(Decimal::ZERO) };
        let status = OrderStatus::Filled { at: Utc::now() };

        let update = OrderUpdate::new(order_id, status, filled_qty, remaining_qty);

        assert!(update.is_filled());
        assert!(update.is_terminal());
    }

    #[test]
    fn order_update_is_not_filled() {
        let order_id = OrderId::generate();
        let filled_qty = Quantity::new(Decimal::new(50, 0)).unwrap();
        let remaining_qty = Quantity::new(Decimal::new(50, 0)).unwrap();
        let status = OrderStatus::PartiallyFilled {
            filled: Quantity::new(Decimal::new(50, 0)).unwrap(),
            remaining: Quantity::new(Decimal::new(50, 0)).unwrap(),
            avg_price: Price::new(Decimal::new(15000, 2)).unwrap(),
        };

        let update = OrderUpdate::new(order_id, status, filled_qty, remaining_qty);

        assert!(!update.is_filled());
        assert!(!update.is_terminal());
    }

    #[test]
    fn order_update_clone() {
        let order_id = OrderId::generate();
        let filled_qty = Quantity::new(Decimal::new(100, 0)).unwrap();
        let remaining_qty = unsafe { Quantity::new_unchecked(Decimal::ZERO) };
        let status = OrderStatus::Filled { at: Utc::now() };

        let update = OrderUpdate::new(order_id, status, filled_qty, remaining_qty);
        let cloned = update.clone();

        assert_eq!(update.order_id, cloned.order_id);
        assert_eq!(update.status, cloned.status);
    }

    // =================================================================================
    // Mock ExecutionGateway Tests
    // =================================================================================

    /// Mock implementation of ExecutionGateway for testing
    struct MockExecutionGateway {
        orders: Arc<Mutex<Vec<Order>>>,
        fills: Arc<Mutex<Vec<Fill>>>,
        positions: Arc<Mutex<Vec<Position>>>,
    }

    impl MockExecutionGateway {
        fn new() -> Self {
            Self {
                orders: Arc::new(Mutex::new(Vec::new())),
                fills: Arc::new(Mutex::new(Vec::new())),
                positions: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl ExecutionGateway for MockExecutionGateway {
        async fn place_order(&self, order: Order) -> Result<OrderId, ExecutionError> {
            let order_id = OrderId::generate();
            self.orders.lock().await.push(order);
            Ok(order_id)
        }

        async fn cancel_order(&self, _order_id: OrderId) -> Result<(), ExecutionError> {
            Ok(())
        }

        async fn modify_order(
            &self,
            _order_id: OrderId,
            _modifications: OrderModifications,
        ) -> Result<(), ExecutionError> {
            Ok(())
        }

        async fn get_order(&self, _order_id: OrderId) -> Result<Option<Order>, ExecutionError> {
            Ok(None)
        }

        async fn get_open_orders(&self) -> Result<Vec<Order>, ExecutionError> {
            Ok(self.orders.lock().await.clone())
        }

        async fn get_positions(&self) -> Result<Vec<Position>, ExecutionError> {
            Ok(self.positions.lock().await.clone())
        }

        async fn subscribe_order_updates(&self) -> Result<mpsc::Receiver<OrderUpdate>, ExecutionError> {
            let (tx, rx) = mpsc::channel(100);
            // Mock: sender is dropped immediately, channel will close
            drop(tx);
            Ok(rx)
        }

        async fn subscribe_fill_updates(&self) -> Result<mpsc::Receiver<Fill>, ExecutionError> {
            let (tx, rx) = mpsc::channel(100);
            drop(tx);
            Ok(rx)
        }
    }

    #[tokio::test]
    async fn mock_gateway_place_order() {
        let gateway = MockExecutionGateway::new();
        let order = create_test_order();

        let order_id = gateway.place_order(order).await;

        assert!(order_id.is_ok());
    }

    #[tokio::test]
    async fn mock_gateway_cancel_order() {
        let gateway = MockExecutionGateway::new();
        let order_id = OrderId::generate();

        let result = gateway.cancel_order(order_id).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn mock_gateway_modify_order() {
        let gateway = MockExecutionGateway::new();
        let order_id = OrderId::generate();
        let modifications = OrderModifications::new()
            .with_quantity(Quantity::new(Decimal::new(200, 0)).unwrap());

        let result = gateway.modify_order(order_id, modifications).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn mock_gateway_get_order() {
        let gateway = MockExecutionGateway::new();
        let order_id = OrderId::generate();

        let result = gateway.get_order(order_id).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn mock_gateway_get_open_orders() {
        let gateway = MockExecutionGateway::new();

        let orders = gateway.get_open_orders().await;

        assert!(orders.is_ok());
        assert!(orders.unwrap().is_empty());
    }

    #[tokio::test]
    async fn mock_gateway_get_positions() {
        let gateway = MockExecutionGateway::new();

        let positions = gateway.get_positions().await;

        assert!(positions.is_ok());
        assert!(positions.unwrap().is_empty());
    }

    #[tokio::test]
    async fn mock_gateway_subscribe_order_updates() {
        let gateway = MockExecutionGateway::new();

        let result = gateway.subscribe_order_updates().await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn mock_gateway_subscribe_fill_updates() {
        let gateway = MockExecutionGateway::new();

        let result = gateway.subscribe_fill_updates().await;

        assert!(result.is_ok());
    }

    // =================================================================================
    // Helper Functions
    // =================================================================================

    fn create_test_order() -> Order {
        use uuid::Uuid;

        Order::new(
            Uuid::new_v4(),
            Symbol::new("AAPL").unwrap(),
            Side::Buy,
            OrderType::Market,
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            None,
            None,
        )
        .unwrap()
    }
}
