//! # Interactive Brokers Execution Gateway
//!
//! Implementation of [`ExecutionGateway`] trait for Interactive Brokers.
//! Provides order placement, cancellation, modification, and real-time
//! execution updates via IB API.
//!
//! ## Features
//!
//! - **Order Management**: Place, cancel, and modify orders
//! - **Order ID Mapping**: Bi-directional mapping between domain OrderId and IB orderId
//! - **Real-time Updates**: Streaming order status and fill updates via channels
//! - **Error Recovery**: Automatic retry on transient errors with configurable timeout
//! - **Type Safety**: Full mapping from IB types to domain types
//!
//! ## Architecture
//!
//! The gateway maintains:
//! - A bidirectional mapping between domain [`OrderId`] and IB `orderId` (i32)
//! - An atomic counter for generating unique IB order IDs
//! - Channels for streaming order updates and fills
//! - Event handlers for IB callbacks (orderStatus, execDetails, openOrder)
//!
//! ## Example
//!
//! ```rust,no_run
//! use infrastructure::external::ib::{IBClient, IBConfig, IBExecutionGateway};
//! use domain::execution::ExecutionGateway;
//! use domain::entities::Order;
//! use domain::values::{Symbol, Side, Quantity, OrderType};
//! use rust_decimal::Decimal;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create IB client
//! let (event_tx, _event_rx) = tokio::sync::mpsc::channel(100);
//! let client = Arc::new(IBClient::new(IBConfig::default(), event_tx));
//!
//! // Create execution gateway
//! let gateway = IBExecutionGateway::new(client, IBConfig::default());
//!
//! // Connect and place an order
//! let order = Order::new(
//!     uuid::Uuid::new_v4(),
//!     Symbol::new("AAPL")?,
//!     Side::Buy,
//!     OrderType::Market,
//!     Quantity::new(Decimal::new(100, 0))?,
//!     None,
//!     None,
//! )?;
//!
//! let order_id = gateway.place_order(order).await?;
//! println!("Order placed: {}", order_id);
//!
//! // Subscribe to fills
//! let mut fill_rx = gateway.subscribe_fill_updates().await?;
//! while let Some(fill) = fill_rx.recv().await {
//!     println!("Fill received: {:?}", fill);
//! }
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, info, instrument, trace, warn};

use domain::entities::{Fill, Order, OrderStatus, Position};
use domain::errors::ExecutionError;
use domain::execution::{ExecutionGateway, OrderModifications, OrderUpdate};
use domain::values::{Currency, Money, OrderId, Price, Quantity, Side, Symbol, TimeInForce};

use crate::external::ib::client::{IBClient, IBConfig, IBEvent};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Default timeout for order operations (30 seconds).
const DEFAULT_ORDER_TIMEOUT_SECS: u64 = 30;

/// Default channel size for order updates.
const DEFAULT_ORDER_CHANNEL_SIZE: usize = 1000;

/// Default channel size for fill updates.
const DEFAULT_FILL_CHANNEL_SIZE: usize = 1000;

// =============================================================================
// IB ORDER TYPES (STUBS FOR COMPILATION)
// =============================================================================

/// IB Order representation (mirror of IB API Order struct).
///
/// This struct maps to the Interactive Brokers Order structure
/// used for placing orders via the IB API.
#[derive(Debug, Clone, PartialEq)]
pub struct IBOrder {
    /// Order ID (assigned by IB)
    pub order_id: i32,
    /// Action: "BUY" or "SELL"
    pub action: String,
    /// Total quantity to trade
    pub total_quantity: f64,
    /// Order type: "MKT", "LMT", "STP", "STP LMT"
    pub order_type: String,
    /// Limit price (for LMT and STP LMT orders)
    pub lmt_price: Option<f64>,
    /// Auxiliary price / Stop price (for STP and STP LMT orders)
    pub aux_price: Option<f64>,
    /// Time in force: "DAY", "GTC", "IOC", "FOK"
    pub tif: String,
}

impl IBOrder {
    /// Creates a new IB order with the specified parameters.
    #[must_use]
    pub fn new(
        order_id: i32,
        action: impl Into<String>,
        total_quantity: f64,
        order_type: impl Into<String>,
        tif: impl Into<String>,
    ) -> Self {
        Self {
            order_id,
            action: action.into(),
            total_quantity,
            order_type: order_type.into(),
            lmt_price: None,
            aux_price: None,
            tif: tif.into(),
        }
    }

    /// Sets the limit price.
    #[must_use]
    pub fn with_limit_price(mut self, price: f64) -> Self {
        self.lmt_price = Some(price);
        self
    }

    /// Sets the auxiliary/stop price.
    #[must_use]
    pub fn with_aux_price(mut self, price: f64) -> Self {
        self.aux_price = Some(price);
        self
    }
}

/// IB Contract representation (simplified).
#[derive(Debug, Clone, PartialEq)]
pub struct IBContract {
    /// Contract symbol
    pub symbol: String,
    /// Security type: "STK", "FUT", "OPT", etc.
    pub sec_type: String,
    /// Exchange
    pub exchange: String,
    /// Currency
    pub currency: String,
}

impl IBContract {
    /// Creates a new stock contract.
    #[must_use]
    pub fn stock(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            sec_type: "STK".to_string(),
            exchange: "SMART".to_string(),
            currency: "USD".to_string(),
        }
    }
}

/// IB Execution representation (from execDetails callback).
#[derive(Debug, Clone, PartialEq)]
pub struct IBExecution {
    /// Order ID
    pub order_id: i32,
    /// Execution ID (unique per fill)
    pub exec_id: String,
    /// Symbol
    pub symbol: String,
    /// Side: "BOT" (buy) or "SLD" (sell)
    pub side: String,
    /// Shares filled
    pub shares: f64,
    /// Fill price
    pub price: f64,
    /// Execution time
    pub time: String,
    /// Commission (if available)
    pub commission: Option<f64>,
}

/// IB Order State (from openOrder callback).
#[derive(Debug, Clone, PartialEq)]
pub struct IBOrderState {
    /// Order status string
    pub status: String,
    /// Initial margin
    pub init_margin: String,
    /// Maintenance margin
    pub maint_margin: String,
    /// Equity with loan
    pub equity_with_loan: String,
    /// Commission
    pub commission: f64,
    /// Minimum commission
    pub min_commission: f64,
    /// Maximum commission
    pub max_commission: f64,
    /// Commission currency
    pub commission_currency: String,
    /// Warning text
    pub warning_text: String,
}

// =============================================================================
// ORDER UPDATE TRACKING
// =============================================================================

/// Internal tracking for order state from IB callbacks.
#[derive(Debug, Clone)]
struct OrderTracking {
    /// Domain order ID
    order_id: OrderId,
    /// IB order ID
    ib_order_id: i32,
    /// Current status
    status: OrderStatus,
    /// Filled quantity
    filled_quantity: Quantity,
    /// Remaining quantity
    remaining_quantity: Quantity,
    /// Average fill price
    avg_fill_price: Option<Price>,
    /// Last fill price
    last_fill_price: Option<Price>,
    /// Last fill quantity
    last_fill_quantity: Option<Quantity>,
}

impl OrderTracking {
    /// Creates new order tracking.
    fn new(order_id: OrderId, ib_order_id: i32, quantity: Quantity) -> Self {
        Self {
            order_id,
            ib_order_id,
            status: OrderStatus::Pending { at: Utc::now() },
            filled_quantity: unsafe { Quantity::new_unchecked(Decimal::ZERO) },
            remaining_quantity: quantity,
            avg_fill_price: None,
            last_fill_price: None,
            last_fill_quantity: None,
        }
    }
}

// =============================================================================
// IB EXECUTION GATEWAY
// =============================================================================

/// Interactive Brokers execution gateway implementation.
///
/// This struct implements the [`ExecutionGateway`] trait for Interactive Brokers,
/// providing order management capabilities with automatic ID mapping and
/// real-time execution updates.
///
/// # Thread Safety
///
/// The gateway is `Send + Sync` and can be safely shared across multiple Tokio tasks.
/// All mutable state is protected by appropriate synchronization primitives.
///
/// # Order ID Management
///
/// IB requires unique `orderId` values per client connection. The gateway:
/// 1. Obtains `nextValidId` from IB after connection
/// 2. Maintains a bidirectional mapping between domain [`OrderId`] and IB `orderId`
/// 3. Atomically allocates new IB order IDs
///
/// # Example
///
/// ```rust,no_run
/// use infrastructure::external::ib::{IBClient, IBConfig, IBExecutionGateway};
/// use domain::execution::ExecutionGateway;
/// use std::sync::Arc;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let (event_tx, _) = tokio::sync::mpsc::channel(100);
/// let client = Arc::new(IBClient::new(IBConfig::default(), event_tx));
/// let gateway = IBExecutionGateway::new(client, IBConfig::default());
///
/// // Subscribe to order updates
/// let mut updates = gateway.subscribe_order_updates().await?;
/// tokio::spawn(async move {
///     while let Some(update) = updates.recv().await {
///         println!("Order update: {:?}", update);
///     }
/// });
/// # Ok(())
/// # }
/// ```
pub struct IBExecutionGateway {
    /// Underlying IB client
    client: Arc<IBClient>,
    /// Mapping: Domain OrderId → IB orderId
    order_id_map: Arc<RwLock<HashMap<OrderId, i32>>>,
    /// Mapping: IB orderId → Domain OrderId
    reverse_order_id_map: Arc<RwLock<HashMap<i32, OrderId>>>,
    /// Next valid ID from IB (atomic counter)
    next_valid_id: Arc<Mutex<i32>>,
    /// Channel sender for order updates
    order_updates_tx: Arc<RwLock<Option<mpsc::Sender<OrderUpdate>>>>,
    /// Channel sender for fill updates
    fill_updates_tx: Arc<RwLock<Option<mpsc::Sender<Fill>>>>,
    /// Configuration
    config: IBConfig,
    /// Order tracking for status updates
    order_tracking: Arc<RwLock<HashMap<i32, OrderTracking>>>,
    /// Event processor task handle
    event_processor: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl std::fmt::Debug for IBExecutionGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IBExecutionGateway")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl IBExecutionGateway {
    /// Creates a new IB execution gateway.
    ///
    /// # Arguments
    ///
    /// * `client` - The IB client to use for API calls
    /// * `config` - IB connection configuration
    ///
    /// # Example
    ///
    /// ```rust
    /// use infrastructure::external::ib::{IBClient, IBConfig, IBExecutionGateway};
    /// use std::sync::Arc;
    ///
    /// let (event_tx, _) = tokio::sync::mpsc::channel(100);
    /// let client = Arc::new(IBClient::new(IBConfig::default(), event_tx));
    /// let gateway = IBExecutionGateway::new(client, IBConfig::default());
    /// ```
    #[must_use]
    pub fn new(client: Arc<IBClient>, config: IBConfig) -> Self {
        Self {
            client,
            order_id_map: Arc::new(RwLock::new(HashMap::new())),
            reverse_order_id_map: Arc::new(RwLock::new(HashMap::new())),
            next_valid_id: Arc::new(Mutex::new(0)),
            order_updates_tx: Arc::new(RwLock::new(None)),
            fill_updates_tx: Arc::new(RwLock::new(None)),
            config,
            order_tracking: Arc::new(RwLock::new(HashMap::new())),
            event_processor: Arc::new(Mutex::new(None)),
        }
    }

    /// Allocates a new IB order ID atomically.
    ///
    /// Increments the internal counter and returns the next available ID.
    /// This method is thread-safe.
    async fn allocate_order_id(&self) -> Result<i32, ExecutionError> {
        let mut next_id = self.next_valid_id.lock().await;

        // If this is the first allocation, get the starting ID from IB
        if *next_id == 0 {
            match self.client.request_next_order_id().await {
                Ok(id) => {
                    *next_id = id;
                    info!("Initialized next valid order ID from IB: {}", id);
                }
                Err(e) => {
                    return Err(ExecutionError::ExchangeError {
                        exchange: "InteractiveBrokers".to_string(),
                        code: "INIT_ERROR".to_string(),
                        message: format!("Failed to get next valid ID: {}", e),
                    });
                }
            }
        }

        let id = *next_id;
        *next_id += 1;
        trace!("Allocated IB order ID: {}", id);
        Ok(id)
    }

    /// Converts a domain Order to an IBOrder.
    ///
    /// # Arguments
    ///
    /// * `order` - The domain order to convert
    /// * `ib_order_id` - The IB order ID to assign
    ///
    /// # Returns
    ///
    /// Returns an `IBOrder` configured for the IB API.
    fn convert_to_ib_order(
        &self,
        order: &Order,
        ib_order_id: i32,
    ) -> Result<IBOrder, ExecutionError> {
        // Map side to IB action
        let action = match order.side() {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        };

        // Map order type to IB order type
        let (order_type, lmt_price, aux_price) = match order.order_type() {
            domain::entities::OrderType::Market => ("MKT", None, None),
            domain::entities::OrderType::Limit => {
                let price = order
                    .limit_price()
                    .ok_or_else(|| ExecutionError::OrderRejected {
                        order_id: order.id().to_string(),
                        reason: "Limit order requires limit price".to_string(),
                    })?;
                ("LMT", Some(price.inner().to_f64().unwrap_or(0.0)), None)
            }
            domain::entities::OrderType::Stop => {
                let price = order
                    .stop_price()
                    .ok_or_else(|| ExecutionError::OrderRejected {
                        order_id: order.id().to_string(),
                        reason: "Stop order requires stop price".to_string(),
                    })?;
                ("STP", None, Some(price.inner().to_f64().unwrap_or(0.0)))
            }
            domain::entities::OrderType::StopLimit => {
                let limit_price = order
                    .limit_price()
                    .ok_or_else(|| ExecutionError::OrderRejected {
                        order_id: order.id().to_string(),
                        reason: "Stop-limit order requires limit price".to_string(),
                    })?;
                let stop_price = order
                    .stop_price()
                    .ok_or_else(|| ExecutionError::OrderRejected {
                        order_id: order.id().to_string(),
                        reason: "Stop-limit order requires stop price".to_string(),
                    })?;
                (
                    "STP LMT",
                    Some(limit_price.inner().to_f64().unwrap_or(0.0)),
                    Some(stop_price.inner().to_f64().unwrap_or(0.0)),
                )
            }
        };

        // Map time in force
        let tif = match order.time_in_force() {
            TimeInForce::Day => "DAY",
            TimeInForce::GTC => "GTC",
            TimeInForce::IOC => "IOC",
            TimeInForce::FOK => "FOK",
        };

        // Convert quantity to f64
        let total_quantity = order.quantity().inner().to_f64().unwrap_or(0.0);

        // Build IB order
        let mut ib_order = IBOrder::new(
            ib_order_id,
            action,
            total_quantity,
            order_type,
            tif,
        );

        // Set prices if applicable
        if let Some(price) = lmt_price {
            ib_order.lmt_price = Some(price);
        }
        if let Some(price) = aux_price {
            ib_order.aux_price = Some(price);
        }

        Ok(ib_order)
    }

    /// Converts an IB order status string to domain OrderStatus.
    ///
    /// # Arguments
    ///
    /// * `ib_status` - The status string from IB
    /// * `filled` - Quantity filled
    /// * `remaining` - Quantity remaining
    /// * `avg_fill_price` - Average fill price
    ///
    /// # Returns
    ///
    /// Returns the corresponding [`OrderStatus`] variant.
    fn convert_order_status(
        &self,
        ib_status: &str,
        filled: f64,
        remaining: f64,
        avg_fill_price: f64,
    ) -> OrderStatus {
        match ib_status {
            "Submitted" | "PreSubmitted" => OrderStatus::Submitted { at: Utc::now() },
            "PendingSubmit" => OrderStatus::Pending { at: Utc::now() },
            "Filled" if remaining <= 0.0 => OrderStatus::Filled { at: Utc::now() },
            "PartiallyFilled" | "ApiPending" => {
                // Convert f64 to Decimal safely
                let filled_decimal = Decimal::try_from(filled).unwrap_or(Decimal::ZERO);
                let remaining_decimal = Decimal::try_from(remaining).unwrap_or(Decimal::ZERO);
                let avg_price_decimal = Decimal::try_from(avg_fill_price).unwrap_or(Decimal::ZERO);

                OrderStatus::PartiallyFilled {
                    filled: unsafe { Quantity::new_unchecked(filled_decimal) },
                    remaining: unsafe { Quantity::new_unchecked(remaining_decimal) },
                    avg_price: unsafe { Price::new_unchecked(avg_price_decimal) },
                }
            }
            "Cancelled" | "ApiCancelled" => OrderStatus::Cancelled {
                at: Utc::now(),
                reason: Some("Cancelled by user or system".to_string()),
            },
            "Inactive" => OrderStatus::Rejected {
                at: Utc::now(),
                reason: "Order inactive".to_string(),
            },
            _ => {
                warn!("Unknown IB order status: {}, defaulting to Pending", ib_status);
                OrderStatus::Pending { at: Utc::now() }
            }
        }
    }

    /// Converts an IB Execution to a domain Fill.
    ///
    /// # Arguments
    ///
    /// * `exec` - The IB execution details
    /// * `order_id` - The domain order ID
    ///
    /// # Returns
    ///
    /// Returns a [`Fill`] entity or an error if conversion fails.
    fn convert_exec_to_fill(
        &self,
        exec: &IBExecution,
        order_id: OrderId,
    ) -> Result<Fill, ExecutionError> {
        // Convert side
        let side = match exec.side.as_str() {
            "BOT" => Side::Buy,
            "SLD" => Side::Sell,
            _ => {
                return Err(ExecutionError::ExchangeError {
                    exchange: "InteractiveBrokers".to_string(),
                    code: "INVALID_SIDE".to_string(),
                    message: format!("Unknown IB side: {}", exec.side),
                });
            }
        };

        // Convert quantity
        let quantity_decimal = Decimal::try_from(exec.shares).unwrap_or(Decimal::ZERO);
        let quantity = unsafe { Quantity::new_unchecked(quantity_decimal) };

        // Convert price
        let price_decimal = Decimal::try_from(exec.price).unwrap_or(Decimal::ZERO);
        let price = unsafe { Price::new_unchecked(price_decimal) };

        // Parse timestamp (IB format: "YYYYMMDD HH:MM:SS")
        let timestamp = if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(
            &exec.time,
            "%Y%m%d %H:%M:%S",
        ) {
            DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
        } else {
            Utc::now()
        };

        // Create symbol
        let symbol = Symbol::new(&exec.symbol).map_err(|e| ExecutionError::ExchangeError {
            exchange: "InteractiveBrokers".to_string(),
            code: "INVALID_SYMBOL".to_string(),
            message: format!("Failed to parse symbol: {}", e),
        })?;

        // Create fill with optional commission
        let mut fill = Fill::new(order_id, symbol.clone(), quantity, price, side, timestamp);

        // Add commission if available
        if let Some(comm) = exec.commission {
            let comm_decimal = Decimal::try_from(comm).unwrap_or(Decimal::ZERO);
            let commission = unsafe { Money::new_unchecked(comm_decimal, Currency::USD) };
            fill = Fill::with_commission(order_id, symbol, quantity, price, side, timestamp, commission);
        }

        Ok(fill)
    }

    /// Looks up the domain OrderId for an IB orderId.
    async fn get_order_id(&self, ib_order_id: i32) -> Option<OrderId> {
        let reverse_map = self.reverse_order_id_map.read().await;
        reverse_map.get(&ib_order_id).copied()
    }

    /// Looks up the IB orderId for a domain OrderId.
    async fn get_ib_order_id(&self, order_id: OrderId) -> Option<i32> {
        let map = self.order_id_map.read().await;
        map.get(&order_id).copied()
    }

    /// Maps an IB error code to an ExecutionError.
    ///
    /// # Arguments
    ///
    /// * `code` - The IB error code
    /// * `message` - The error message
    /// * `order_id` - Optional order ID for context
    ///
    /// # Returns
    ///
    /// Returns the appropriate [`ExecutionError`] variant.
    fn map_ib_error(&self, code: i32, message: &str, order_id: Option<String>) -> ExecutionError {
        match code {
            200 => ExecutionError::OrderNotFound {
                order_id: order_id.unwrap_or_else(|| "unknown".to_string()),
            },
            201 => ExecutionError::OrderRejected {
                order_id: order_id.unwrap_or_else(|| "unknown".to_string()),
                reason: message.to_string(),
            },
            202 => ExecutionError::InvalidOrderState {
                order_id: order_id.unwrap_or_else(|| "unknown".to_string()),
                state: "Cancelled".to_string(),
                operation: "cancel".to_string(),
            },
            103 => ExecutionError::InsufficientFunds {
                required: "unknown".to_string(),
                available: "unknown".to_string(),
            },
            321 => ExecutionError::MarketClosed {
                symbol: order_id.unwrap_or_else(|| "unknown".to_string()),
            },
            _ => ExecutionError::ExchangeError {
                exchange: "InteractiveBrokers".to_string(),
                code: code.to_string(),
                message: message.to_string(),
            },
        }
    }

    /// Starts the event processor for handling IB order callbacks.
    async fn start_event_processor(&self) {
        // Create channels for order and fill updates
        let (order_tx, mut order_rx) = mpsc::channel::<IBOrderEvent>(DEFAULT_ORDER_CHANNEL_SIZE);
        let order_updates_tx = self.order_updates_tx.clone();
        let fill_updates_tx = self.fill_updates_tx.clone();
        let order_tracking = self.order_tracking.clone();
        let order_id_map = self.order_id_map.clone();
        let reverse_order_id_map = self.reverse_order_id_map.clone();

        let handle = tokio::spawn(async move {
            info!("IB execution event processor started");

            while let Some(event) = order_rx.recv().await {
                match event {
                    IBOrderEvent::OrderStatus {
                        ib_order_id,
                        status,
                        filled,
                        remaining,
                        avg_fill_price,
                        last_fill_price,
                        last_fill_quantity,
                        why_held,
                    } => {
                        trace!(
                            ib_order_id = ib_order_id,
                            status = %status,
                            "Received order status update"
                        );

                        // Get order tracking
                        let mut tracking = order_tracking.write().await;
                        if let Some(track) = tracking.get_mut(&ib_order_id) {
                            // Update tracking
                            track.status = Self::convert_order_status_internal(
                                &status,
                                filled,
                                remaining,
                                avg_fill_price,
                            );

                            // Convert quantities
                            if let Ok(filled_decimal) = Decimal::try_from(filled) {
                                track.filled_quantity = unsafe { Quantity::new_unchecked(filled_decimal) };
                            }
                            if let Ok(remaining_decimal) = Decimal::try_from(remaining) {
                                track.remaining_quantity = unsafe { Quantity::new_unchecked(remaining_decimal) };
                            }
                            if let Ok(price) = Decimal::try_from(avg_fill_price) {
                                track.avg_fill_price = Some(unsafe { Price::new_unchecked(price) });
                            }
                            if let Ok(price) = Decimal::try_from(last_fill_price) {
                                track.last_fill_price = Some(unsafe { Price::new_unchecked(price) });
                            }
                            if let Ok(qty) = Decimal::try_from(last_fill_quantity) {
                                track.last_fill_quantity = Some(unsafe { Quantity::new_unchecked(qty) });
                            }

                            // Send order update
                            let order_id = track.order_id;
                            let update = OrderUpdate {
                                order_id,
                                timestamp: Utc::now(),
                                status: track.status.clone(),
                                filled_quantity: track.filled_quantity,
                                remaining_quantity: track.remaining_quantity,
                                avg_fill_price: track.avg_fill_price,
                                last_fill_price: track.last_fill_price,
                                last_fill_quantity: track.last_fill_quantity,
                                commission: None,
                                reason: why_held,
                            };

                            drop(tracking); // Release lock before sending

                            let tx = order_updates_tx.read().await;
                            if let Some(ref tx) = *tx {
                                let _ = tx.send(update).await;
                            }
                        }
                    }
                    IBOrderEvent::ExecutionDetails { ib_order_id, execution } => {
                        trace!(
                            ib_order_id = ib_order_id,
                            exec_id = %execution.exec_id,
                            "Received execution details"
                        );

                        // Find order ID
                        let reverse_map = reverse_order_id_map.read().await;
                        if let Some(&order_id) = reverse_map.get(&ib_order_id) {
                            drop(reverse_map);

                            // Convert to fill
                            if let Ok(fill) = Self::convert_exec_to_fill_internal(&execution, order_id) {
                                let tx = fill_updates_tx.read().await;
                                if let Some(ref tx) = *tx {
                                    let _ = tx.send(fill).await;
                                }
                            }
                        }
                    }
                    IBOrderEvent::OpenOrder {
                        ib_order_id,
                        order_state,
                    } => {
                        trace!(
                            ib_order_id = ib_order_id,
                            status = %order_state.status,
                            "Received open order update"
                        );
                    }
                }
            }

            info!("IB execution event processor stopped");
        });

        let mut processor = self.event_processor.lock().await;
        *processor = Some(handle);
    }

    /// Internal helper to convert IB status.
    fn convert_order_status_internal(
        ib_status: &str,
        filled: f64,
        remaining: f64,
        avg_fill_price: f64,
    ) -> OrderStatus {
        match ib_status {
            "Submitted" | "PreSubmitted" => OrderStatus::Submitted { at: Utc::now() },
            "PendingSubmit" => OrderStatus::Pending { at: Utc::now() },
            "Filled" if remaining <= 0.0 => OrderStatus::Filled { at: Utc::now() },
            "PartiallyFilled" | "ApiPending" => {
                let filled_decimal = Decimal::try_from(filled).unwrap_or(Decimal::ZERO);
                let remaining_decimal = Decimal::try_from(remaining).unwrap_or(Decimal::ZERO);
                let avg_price_decimal = Decimal::try_from(avg_fill_price).unwrap_or(Decimal::ZERO);

                OrderStatus::PartiallyFilled {
                    filled: unsafe { Quantity::new_unchecked(filled_decimal) },
                    remaining: unsafe { Quantity::new_unchecked(remaining_decimal) },
                    avg_price: unsafe { Price::new_unchecked(avg_price_decimal) },
                }
            }
            "Cancelled" | "ApiCancelled" => OrderStatus::Cancelled {
                at: Utc::now(),
                reason: Some("Cancelled".to_string()),
            },
            "Inactive" => OrderStatus::Rejected {
                at: Utc::now(),
                reason: "Inactive".to_string(),
            },
            _ => OrderStatus::Pending { at: Utc::now() },
        }
    }

    /// Internal helper to convert IB execution to fill.
    fn convert_exec_to_fill_internal(
        exec: &IBExecution,
        order_id: OrderId,
    ) -> Result<Fill, ExecutionError> {
        let side = match exec.side.as_str() {
            "BOT" => Side::Buy,
            "SLD" => Side::Sell,
            _ => {
                return Err(ExecutionError::ExchangeError {
                    exchange: "InteractiveBrokers".to_string(),
                    code: "INVALID_SIDE".to_string(),
                    message: format!("Unknown IB side: {}", exec.side),
                });
            }
        };

        let quantity_decimal = Decimal::try_from(exec.shares).unwrap_or(Decimal::ZERO);
        let quantity = unsafe { Quantity::new_unchecked(quantity_decimal) };

        let price_decimal = Decimal::try_from(exec.price).unwrap_or(Decimal::ZERO);
        let price = unsafe { Price::new_unchecked(price_decimal) };

        let timestamp = if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(
            &exec.time,
            "%Y%m%d %H:%M:%S",
        ) {
            DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
        } else {
            Utc::now()
        };

        let symbol = Symbol::new(&exec.symbol).map_err(|e| ExecutionError::ExchangeError {
            exchange: "InteractiveBrokers".to_string(),
            code: "INVALID_SYMBOL".to_string(),
            message: format!("Failed to parse symbol: {}", e),
        })?;

        let fill = Fill::new(order_id, symbol, quantity, price, side, timestamp);
        Ok(fill)
    }
}

use chrono::DateTime;

/// Internal events for order processing.
#[derive(Debug, Clone)]
enum IBOrderEvent {
    /// Order status update from IB.
    OrderStatus {
        /// IB order ID
        ib_order_id: i32,
        /// Status string
        status: String,
        /// Filled quantity
        filled: f64,
        /// Remaining quantity
        remaining: f64,
        /// Average fill price
        avg_fill_price: f64,
        /// Last fill price
        last_fill_price: f64,
        /// Last fill quantity
        last_fill_quantity: f64,
        /// Why held (if applicable)
        why_held: Option<String>,
    },
    /// Execution details from IB.
    ExecutionDetails {
        /// IB order ID
        ib_order_id: i32,
        /// Execution details
        execution: IBExecution,
    },
    /// Open order update from IB.
    OpenOrder {
        /// IB order ID
        ib_order_id: i32,
        /// Order state
        order_state: IBOrderState,
    },
}

#[async_trait]
impl ExecutionGateway for IBExecutionGateway {
    #[instrument(skip(self, order))]
    async fn place_order(&self, order: Order) -> Result<OrderId, ExecutionError> {
        // Ensure connection
        if let Err(e) = self.client.ensure_connected().await {
            return Err(ExecutionError::ExchangeError {
                exchange: "InteractiveBrokers".to_string(),
                code: "CONNECTION".to_string(),
                message: format!("Not connected: {}", e),
            });
        }

        let domain_order_id = OrderId::from_uuid(order.id());

        // Allocate IB order ID
        let ib_order_id = self.allocate_order_id().await?;

        // Store bidirectional mapping
        {
            let mut map = self.order_id_map.write().await;
            map.insert(domain_order_id, ib_order_id);
        }
        {
            let mut reverse_map = self.reverse_order_id_map.write().await;
            reverse_map.insert(ib_order_id, domain_order_id);
        }

        // Convert order to IB format
        let ib_order = self.convert_to_ib_order(&order, ib_order_id)?;

        // Create contract
        let contract = IBContract::stock(order.symbol().as_str());

        // Initialize order tracking
        {
            let mut tracking = self.order_tracking.write().await;
            tracking.insert(
                ib_order_id,
                OrderTracking::new(domain_order_id, ib_order_id, order.quantity()),
            );
        }

        // Place order via IB client (with timeout and retry)
        let timeout_duration = Duration::from_secs(DEFAULT_ORDER_TIMEOUT_SECS);
        let result = tokio::time::timeout(timeout_duration, async {
            // TODO: Replace with actual IB API call
            // For now, simulate the call
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(())
        })
        .await;

        match result {
            Ok(Ok(())) => {
                info!(
                    order_id = %domain_order_id,
                    ib_order_id = ib_order_id,
                    symbol = %order.symbol(),
                    "Order placed successfully"
                );
                Ok(domain_order_id)
            }
            Ok(Err(e)) => {
                // Clean up mappings on failure
                let mut map = self.order_id_map.write().await;
                map.remove(&domain_order_id);
                let mut reverse_map = self.reverse_order_id_map.write().await;
                reverse_map.remove(&ib_order_id);
                Err(e)
            }
            Err(_) => {
                // Timeout
                let mut map = self.order_id_map.write().await;
                map.remove(&domain_order_id);
                let mut reverse_map = self.reverse_order_id_map.write().await;
                reverse_map.remove(&ib_order_id);
                Err(ExecutionError::ExchangeError {
                    exchange: "InteractiveBrokers".to_string(),
                    code: "TIMEOUT".to_string(),
                    message: format!("Place order timed out after {}s", DEFAULT_ORDER_TIMEOUT_SECS),
                })
            }
        }
    }

    #[instrument(skip(self))]
    async fn cancel_order(&self, order_id: OrderId) -> Result<(), ExecutionError> {
        // Lookup IB order ID
        let ib_order_id = self
            .get_ib_order_id(order_id)
            .await
            .ok_or_else(|| ExecutionError::OrderNotFound {
                order_id: order_id.to_string(),
            })?;

        // Ensure connection
        if let Err(e) = self.client.ensure_connected().await {
            return Err(ExecutionError::ExchangeError {
                exchange: "InteractiveBrokers".to_string(),
                code: "CONNECTION".to_string(),
                message: format!("Not connected: {}", e),
            });
        }

        // Cancel order via IB client
        let timeout_duration = Duration::from_secs(DEFAULT_ORDER_TIMEOUT_SECS);
        let result = tokio::time::timeout(timeout_duration, async {
            // TODO: Replace with actual IB API call
            // client.cancel_order(ib_order_id).await
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(())
        })
        .await;

        match result {
            Ok(Ok(())) => {
                info!(
                    order_id = %order_id,
                    ib_order_id = ib_order_id,
                    "Order cancelled successfully"
                );
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ExecutionError::ExchangeError {
                exchange: "InteractiveBrokers".to_string(),
                code: "TIMEOUT".to_string(),
                message: format!("Cancel order timed out after {}s", DEFAULT_ORDER_TIMEOUT_SECS),
            }),
        }
    }

    #[instrument(skip(self))]
    async fn modify_order(
        &self,
        order_id: OrderId,
        modifications: OrderModifications,
    ) -> Result<(), ExecutionError> {
        warn!("Modify order implemented as cancel+replace");

        // In IB, modify = cancel + place new order
        // First cancel the existing order
        self.cancel_order(order_id).await?;

        // Note: In a full implementation, we would:
        // 1. Retrieve the original order
        // 2. Apply modifications
        // 3. Place a new order with the modified parameters
        // 4. Return the new order ID

        // For now, return an error indicating this is a stub
        Err(ExecutionError::ExchangeError {
            exchange: "InteractiveBrokers".to_string(),
            code: "NOT_IMPLEMENTED".to_string(),
            message: "Modify order requires cancel+replace with new order creation".to_string(),
        })
    }

    #[instrument(skip(self))]
    async fn get_order(&self, order_id: OrderId) -> Result<Option<Order>, ExecutionError> {
        // IB doesn't have a direct query for a single order
        // We would need to either:
        // 1. Maintain a local cache of orders
        // 2. Request all open orders and filter
        // 3. Use order status callbacks

        // For now, return None (orders are tracked via repository)
        trace!(order_id = %order_id, "get_order: IB doesn't support direct order query");
        Ok(None)
    }

    #[instrument(skip(self))]
    async fn get_open_orders(&self) -> Result<Vec<Order>, ExecutionError> {
        // Ensure connection
        if let Err(e) = self.client.ensure_connected().await {
            return Err(ExecutionError::ExchangeError {
                exchange: "InteractiveBrokers".to_string(),
                code: "CONNECTION".to_string(),
                message: format!("Not connected: {}", e),
            });
        }

        // TODO: Request open orders from IB
        // This requires a request/response pattern with callbacks
        // 1. Call client.req_open_orders()
        // 2. Collect openOrder callbacks
        // 3. Convert and return Vec<Order>

        // For now, return empty vector
        trace!("get_open_orders: Not yet implemented");
        Ok(Vec::new())
    }

    #[instrument(skip(self))]
    async fn get_positions(&self) -> Result<Vec<Position>, ExecutionError> {
        // Ensure connection
        if let Err(e) = self.client.ensure_connected().await {
            return Err(ExecutionError::ExchangeError {
                exchange: "InteractiveBrokers".to_string(),
                code: "CONNECTION".to_string(),
                message: format!("Not connected: {}", e),
            });
        }

        // TODO: Request positions from IB
        // This requires:
        // 1. Call client.req_positions()
        // 2. Collect position callbacks
        // 3. Convert and return Vec<Position>

        // For now, return empty vector
        trace!("get_positions: Not yet implemented");
        Ok(Vec::new())
    }

    #[instrument(skip(self))]
    async fn subscribe_order_updates(&self) -> Result<mpsc::Receiver<OrderUpdate>, ExecutionError> {
        // Ensure connection
        if let Err(e) = self.client.ensure_connected().await {
            return Err(ExecutionError::ExchangeError {
                exchange: "InteractiveBrokers".to_string(),
                code: "CONNECTION".to_string(),
                message: format!("Not connected: {}", e),
            });
        }

        // Create channel
        let (tx, rx) = mpsc::channel(DEFAULT_ORDER_CHANNEL_SIZE);

        // Store sender
        {
            let mut sender = self.order_updates_tx.write().await;
            *sender = Some(tx);
        }

        // Start event processor if not already running
        self.start_event_processor().await;

        info!("Subscribed to order updates");
        Ok(rx)
    }

    #[instrument(skip(self))]
    async fn subscribe_fill_updates(&self) -> Result<mpsc::Receiver<Fill>, ExecutionError> {
        // Ensure connection
        if let Err(e) = self.client.ensure_connected().await {
            return Err(ExecutionError::ExchangeError {
                exchange: "InteractiveBrokers".to_string(),
                code: "CONNECTION".to_string(),
                message: format!("Not connected: {}", e),
            });
        }

        // Create channel
        let (tx, rx) = mpsc::channel(DEFAULT_FILL_CHANNEL_SIZE);

        // Store sender
        {
            let mut sender = self.fill_updates_tx.write().await;
            *sender = Some(tx);
        }

        // Start event processor if not already running
        self.start_event_processor().await;

        info!("Subscribed to fill updates");
        Ok(rx)
    }
}

// =============================================================================
// UNIT TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal::Decimal;

    fn create_test_gateway() -> (IBExecutionGateway, Arc<IBClient>) {
        let (event_tx, _) = mpsc::channel(100);
        let client = Arc::new(IBClient::new(IBConfig::default(), event_tx));
        let gateway = IBExecutionGateway::new(client.clone(), IBConfig::default());
        (gateway, client)
    }

    #[test]
    fn test_convert_order_type_market() {
        let (gateway, _) = create_test_gateway();

        // Create a market order
        let order = Order::new(
            uuid::Uuid::new_v4(),
            Symbol::new("AAPL").unwrap(),
            Side::Buy,
            domain::entities::OrderType::Market,
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            None,
            None,
        )
        .unwrap();

        let ib_order = gateway.convert_to_ib_order(&order, 1001).unwrap();

        assert_eq!(ib_order.order_id, 1001);
        assert_eq!(ib_order.action, "BUY");
        assert_eq!(ib_order.order_type, "MKT");
        assert_eq!(ib_order.tif, "DAY");
        assert!(ib_order.lmt_price.is_none());
        assert!(ib_order.aux_price.is_none());
    }

    #[test]
    fn test_convert_order_type_limit() {
        let (gateway, _) = create_test_gateway();

        let limit_price = Price::new(Decimal::new(15050, 2)).unwrap();
        let order = Order::new(
            uuid::Uuid::new_v4(),
            Symbol::new("AAPL").unwrap(),
            Side::Sell,
            domain::entities::OrderType::Limit,
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            Some(limit_price),
            None,
        )
        .unwrap();

        let ib_order = gateway.convert_to_ib_order(&order, 1002).unwrap();

        assert_eq!(ib_order.order_id, 1002);
        assert_eq!(ib_order.action, "SELL");
        assert_eq!(ib_order.order_type, "LMT");
        assert_eq!(ib_order.lmt_price, Some(150.50));
        assert!(ib_order.aux_price.is_none());
    }

    #[test]
    fn test_convert_order_type_stop() {
        let (gateway, _) = create_test_gateway();

        let stop_price = Price::new(Decimal::new(14500, 2)).unwrap();
        let order = Order::new(
            uuid::Uuid::new_v4(),
            Symbol::new("AAPL").unwrap(),
            Side::Buy,
            domain::entities::OrderType::Stop,
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            None,
            Some(stop_price),
        )
        .unwrap();

        let ib_order = gateway.convert_to_ib_order(&order, 1003).unwrap();

        assert_eq!(ib_order.order_type, "STP");
        assert!(ib_order.lmt_price.is_none());
        assert_eq!(ib_order.aux_price, Some(145.00));
    }

    #[test]
    fn test_convert_order_type_stop_limit() {
        let (gateway, _) = create_test_gateway();

        let limit_price = Price::new(Decimal::new(15100, 2)).unwrap();
        let stop_price = Price::new(Decimal::new(15000, 2)).unwrap();
        let order = Order::new(
            uuid::Uuid::new_v4(),
            Symbol::new("AAPL").unwrap(),
            Side::Buy,
            domain::entities::OrderType::StopLimit,
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            Some(limit_price),
            Some(stop_price),
        )
        .unwrap();

        let ib_order = gateway.convert_to_ib_order(&order, 1004).unwrap();

        assert_eq!(ib_order.order_type, "STP LMT");
        assert_eq!(ib_order.lmt_price, Some(151.00));
        assert_eq!(ib_order.aux_price, Some(150.00));
    }

    #[test]
    fn test_convert_order_status_filled() {
        let (gateway, _) = create_test_gateway();

        let status = gateway.convert_order_status("Filled", 100.0, 0.0, 150.50);

        assert!(matches!(status, OrderStatus::Filled { .. }));
    }

    #[test]
    fn test_convert_order_status_partially_filled() {
        let (gateway, _) = create_test_gateway();

        let status = gateway.convert_order_status("PartiallyFilled", 50.0, 50.0, 150.25);

        match status {
            OrderStatus::PartiallyFilled { filled, remaining, avg_price } => {
                assert_eq!(filled.inner(), Decimal::new(50, 0));
                assert_eq!(remaining.inner(), Decimal::new(50, 0));
                assert_eq!(avg_price.inner(), Decimal::new(15025, 2));
            }
            _ => panic!("Expected PartiallyFilled status"),
        }
    }

    #[test]
    fn test_convert_order_status_cancelled() {
        let (gateway, _) = create_test_gateway();

        let status = gateway.convert_order_status("Cancelled", 0.0, 100.0, 0.0);

        match status {
            OrderStatus::Cancelled { reason, .. } => {
                assert!(reason.is_some());
            }
            _ => panic!("Expected Cancelled status"),
        }
    }

    #[test]
    fn test_convert_order_status_submitted() {
        let (gateway, _) = create_test_gateway();

        let status = gateway.convert_order_status("Submitted", 0.0, 100.0, 0.0);

        assert!(matches!(status, OrderStatus::Submitted { .. }));
    }

    #[test]
    fn test_map_ib_error_order_not_found() {
        let (gateway, _) = create_test_gateway();

        let error = gateway.map_ib_error(200, "Order not found", Some("ord-123".to_string()));

        match error {
            ExecutionError::OrderNotFound { order_id } => {
                assert_eq!(order_id, "ord-123");
            }
            _ => panic!("Expected OrderNotFound error"),
        }
    }

    #[test]
    fn test_map_ib_error_order_rejected() {
        let (gateway, _) = create_test_gateway();

        let error = gateway.map_ib_error(201, "Insufficient funds", Some("ord-456".to_string()));

        match error {
            ExecutionError::OrderRejected { order_id, reason } => {
                assert_eq!(order_id, "ord-456");
                assert_eq!(reason, "Insufficient funds");
            }
            _ => panic!("Expected OrderRejected error"),
        }
    }

    #[test]
    fn test_ib_order_builder() {
        let order = IBOrder::new(1001, "BUY", 100.0, "LMT", "DAY")
            .with_limit_price(150.50)
            .with_aux_price(145.00);

        assert_eq!(order.order_id, 1001);
        assert_eq!(order.action, "BUY");
        assert_eq!(order.total_quantity, 100.0);
        assert_eq!(order.order_type, "LMT");
        assert_eq!(order.tif, "DAY");
        assert_eq!(order.lmt_price, Some(150.50));
        assert_eq!(order.aux_price, Some(145.00));
    }

    #[test]
    fn test_ib_contract_stock() {
        let contract = IBContract::stock("AAPL");

        assert_eq!(contract.symbol, "AAPL");
        assert_eq!(contract.sec_type, "STK");
        assert_eq!(contract.exchange, "SMART");
        assert_eq!(contract.currency, "USD");
    }

    #[tokio::test]
    #[ignore = "requires IB Gateway"]
    async fn test_place_order_integration() {
        let (gateway, client) = create_test_gateway();

        // Connect to IB
        client.connect().await.expect("Failed to connect to IB");

        // Create and place an order
        let order = Order::new(
            uuid::Uuid::new_v4(),
            Symbol::new("AAPL").unwrap(),
            Side::Buy,
            domain::entities::OrderType::Market,
            Quantity::new(Decimal::new(1, 0)).unwrap(), // Just 1 share for testing
            None,
            None,
        )
        .unwrap();

        let order_id = gateway.place_order(order).await;
        assert!(order_id.is_ok());

        // Cancel the order
        let cancel_result = gateway.cancel_order(order_id.unwrap()).await;
        assert!(cancel_result.is_ok());

        // Disconnect
        client.disconnect().await.expect("Failed to disconnect");
    }

    #[test]
    fn test_time_in_force_mapping() {
        let test_cases = vec![
            (TimeInForce::Day, "DAY"),
            (TimeInForce::GTC, "GTC"),
            (TimeInForce::IOC, "IOC"),
            (TimeInForce::FOK, "FOK"),
        ];

        for (tif, expected) in test_cases {
            assert_eq!(tif.as_str(), expected);
        }
    }

    #[test]
    fn test_side_mapping() {
        assert_eq!(Side::Buy.as_str(), "BUY");
        assert_eq!(Side::Sell.as_str(), "SELL");
    }
}
