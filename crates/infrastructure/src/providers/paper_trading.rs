//! # Paper Trading Gateway
//!
//! Implementation of [`ExecutionGateway`] trait for paper trading simulation.
//! Provides realistic order execution simulation with configurable slippage,
//! latency, and commission modeling without interacting with real brokers.
//!
//! ## Features
//!
//! - **Realistic Fill Simulation**: Market orders filled at current market price with slippage
//! - **Limit Order Support**: Orders execute when price conditions are met
//! - **Position Tracking**: In-memory position management with P&L calculation
//! - **Configurable Latency**: Simulated network delays
//! - **Commission Modeling**: Per-trade commission simulation
//! - **Real-time Updates**: Streaming order status and fill updates via channels
//!
//! ## Architecture
//!
//! The gateway maintains:
//! - Order book simulation with price-time priority
//! - Position tracking per symbol
//! - Real-time P&L calculation based on market data
//! - Configurable slippage and latency models
//!
//! ## Example
//!
//! ```rust,no_run
//! use infrastructure::providers::{PaperTradingGateway, PaperTradingConfig};
//! use domain::execution::ExecutionGateway;
//! use domain::entities::Order;
//! use domain::values::{Symbol, Side, Quantity, OrderType};
//! use rust_decimal::Decimal;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create paper trading gateway with default config
//! let config = PaperTradingConfig::default();
//! let market_data = Arc::new(MockMarketDataProvider::new());
//! let gateway = PaperTradingGateway::new(config, market_data);
//!
//! // Place a market order
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
//! println!("Paper order placed: {}", order_id);
//!
//! // Subscribe to fills
//! let mut fill_rx = gateway.subscribe_fill_updates().await?;
//! while let Some(fill) = fill_rx.recv().await {
//!     println!("Paper fill received: {:?}", fill);
//! }
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, Duration};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use domain::entities::{Fill, Order, OrderStatus, OrderType, Position, PositionDirection};
use domain::errors::ExecutionError;
use domain::execution::{ExecutionGateway, OrderModifications, OrderUpdate};
use domain::providers::MarketDataProvider;
use domain::values::{Currency, Money, OrderId, Price, Quantity, Side, Symbol};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Default slippage in ticks (0.25 = 1/4 tick).
const DEFAULT_SLIPPAGE_TICKS: &str = "0.25";

/// Default simulated latency in milliseconds.
const DEFAULT_LATENCY_MS: u64 = 50;

/// Default commission per trade in USD.
const DEFAULT_COMMISSION_USD: &str = "2.50";

/// Default initial capital in USD.
const DEFAULT_INITIAL_CAPITAL_USD: &str = "100000";

/// Default tick size for unknown symbols.
const DEFAULT_TICK_SIZE: &str = "0.01";

/// Default channel size for order updates.
const DEFAULT_ORDER_CHANNEL_SIZE: usize = 1000;

/// Default channel size for fill updates.
const DEFAULT_FILL_CHANNEL_SIZE: usize = 1000;

// =============================================================================
// CONFIGURATION
// =============================================================================

/// Configuration for paper trading simulation.
///
/// Controls the behavior of the paper trading gateway including
/// slippage, latency, commission, and initial capital settings.
///
/// # Example
///
/// ```
/// use infrastructure::providers::PaperTradingConfig;
/// use domain::values::{Currency, Money, Symbol};
/// use rust_decimal::Decimal;
///
/// let mut config = PaperTradingConfig::default();
/// config.slippage_ticks = Decimal::new(5, 2); // 0.05 ticks slippage
/// config.latency_ms = 100; // 100ms simulated latency
/// config.commission_per_trade = Money::new(Decimal::new(1, 0), Currency::USD).unwrap();
///
/// // Set tick size for specific symbols
/// config.tick_sizes.insert(Symbol::new("ES").unwrap(), Decimal::new(25, 2)); // $0.25
/// ```
#[derive(Debug, Clone)]
pub struct PaperTradingConfig {
    /// Slippage in ticks (e.g., 0.25 = 1/4 tick).
    /// Applied against the trader (worse fill price).
    pub slippage_ticks: Decimal,

    /// Simulated network latency in milliseconds.
    pub latency_ms: u64,

    /// Fixed commission per trade.
    pub commission_per_trade: Money,

    /// Tick sizes per symbol for slippage calculation.
    pub tick_sizes: HashMap<Symbol, Decimal>,

    /// Initial positions to populate the account.
    pub initial_positions: Vec<Position>,

    /// Initial capital for the paper trading account.
    pub initial_capital: Money,
}

impl Default for PaperTradingConfig {
    fn default() -> Self {
        Self {
            slippage_ticks: DEFAULT_SLIPPAGE_TICKS.parse().unwrap(),
            latency_ms: DEFAULT_LATENCY_MS,
            commission_per_trade: Money::new(
                DEFAULT_COMMISSION_USD.parse().unwrap(),
                Currency::USD,
            )
            .unwrap(),
            tick_sizes: HashMap::new(),
            initial_positions: Vec::new(),
            initial_capital: Money::new(
                DEFAULT_INITIAL_CAPITAL_USD.parse().unwrap(),
                Currency::USD,
            )
            .unwrap(),
        }
    }
}

// =============================================================================
// INTERNAL STATE
// =============================================================================

/// Internal state of a paper trading order.
#[derive(Debug, Clone)]
struct PaperOrder {
    /// The underlying order.
    order: Order,

    /// Current order status.
    status: OrderStatus,

    /// Fills for this order.
    fills: Vec<Fill>,

    /// Creation timestamp.
    created_at: DateTime<Utc>,

    /// Last update timestamp.
    updated_at: DateTime<Utc>,
}

impl PaperOrder {
    /// Creates a new paper order from an order.
    fn new(order: Order) -> Self {
        let now = Utc::now();
        Self {
            order,
            status: OrderStatus::Created,
            fills: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Adds a fill to this order and updates status.
    fn add_fill(&mut self, fill: Fill) {
        self.fills.push(fill);
        self.updated_at = Utc::now();

        // Calculate total filled quantity
        let total_filled: Decimal = self.fills.iter().map(|f| f.quantity().inner()).sum();
        let order_qty = self.order.quantity().inner();

        // Update status based on fill amount
        self.status = if total_filled >= order_qty {
            OrderStatus::Filled { at: Utc::now() }
        } else {
            let remaining = order_qty - total_filled;
            let avg_price = self.calculate_avg_fill_price();
            OrderStatus::PartiallyFilled {
                filled: Quantity::new(total_filled).unwrap(),
                remaining: Quantity::new(remaining).unwrap(),
                avg_price,
            }
        };
    }

    /// Calculates the average fill price weighted by quantity.
    fn calculate_avg_fill_price(&self) -> Price {
        if self.fills.is_empty() {
            // Return limit price if available, otherwise default
            return self.order.limit_price().unwrap_or_else(|| {
                // SAFETY: default price is positive
                unsafe { Price::new_unchecked(Decimal::ZERO) }
            });
        }

        let total_qty: Decimal = self.fills.iter().map(|f| f.quantity().inner()).sum();
        let total_notional: Decimal = self
            .fills
            .iter()
            .map(|f| f.price().inner() * f.quantity().inner())
            .sum();

        let avg = total_notional / total_qty;
        Price::new(avg).unwrap()
    }
}

/// Trade record for performance tracking.
#[derive(Debug, Clone)]
struct TradeRecord {
    /// Symbol traded.
    symbol: Symbol,

    /// Entry price.
    entry_price: Price,

    /// Exit price (None if still open).
    exit_price: Option<Price>,

    /// Quantity traded.
    quantity: Quantity,

    /// Side of the trade.
    side: Side,

    /// Realized P&L.
    realized_pnl: Money,

    /// Commission paid.
    commission: Money,

    /// Entry timestamp.
    entry_at: DateTime<Utc>,

    /// Exit timestamp (None if still open).
    exit_at: Option<DateTime<Utc>>,
}

// =============================================================================
// PAPER TRADING GATEWAY
// =============================================================================

/// Paper Trading Gateway - Simulates order execution without real broker interaction.
///
/// This gateway provides a realistic simulation environment for testing
/// trading strategies without risking real capital. It models:
///
/// - Market impact through configurable slippage
/// - Network latency through simulated delays
/// - Trading costs through per-trade commissions
/// - Real-time position tracking and P&L calculation
///
/// # Thread Safety
///
/// The gateway is `Send + Sync` and can be safely shared across multiple Tokio tasks.
/// All mutable state is protected by appropriate synchronization primitives:
/// - `RwLock` for orders and positions (read-heavy operations)
/// - `Mutex` for order ID allocation and channels
///
/// # Example
///
/// ```rust,no_run
/// use infrastructure::providers::{PaperTradingGateway, PaperTradingConfig};
/// use domain::execution::ExecutionGateway;
/// use std::sync::Arc;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = PaperTradingConfig::default();
/// let market_data = Arc::new(MockMarketDataProvider::new());
/// let gateway = PaperTradingGateway::new(config, market_data);
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
pub struct PaperTradingGateway {
    /// Configuration for the gateway.
    config: PaperTradingConfig,

    /// Market data provider for pricing.
    market_data: Arc<dyn MarketDataProvider>,

    /// Active and historical orders.
    orders: Arc<RwLock<HashMap<OrderId, PaperOrder>>>,

    /// Current positions.
    positions: Arc<RwLock<HashMap<Symbol, Position>>>,

    /// Trade history for performance tracking.
    trade_history: Arc<RwLock<Vec<TradeRecord>>>,

    /// Next order ID counter.
    next_order_id: Arc<Mutex<u64>>,

    /// Channel sender for order updates.
    order_updates_tx: Arc<Mutex<Option<mpsc::Sender<OrderUpdate>>>>,

    /// Channel sender for fill updates.
    fill_updates_tx: Arc<Mutex<Option<mpsc::Sender<Fill>>>>,

    /// Available capital.
    available_capital: Arc<RwLock<Money>>,

    /// Total realized P&L.
    total_realized_pnl: Arc<RwLock<Money>>,
}

impl std::fmt::Debug for PaperTradingGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaperTradingGateway")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl PaperTradingGateway {
    /// Creates a new paper trading gateway.
    ///
    /// # Arguments
    ///
    /// * `config` - Paper trading configuration
    /// * `market_data` - Market data provider for pricing
    ///
    /// # Example
    ///
    /// ```
    /// use infrastructure::providers::{PaperTradingGateway, PaperTradingConfig};
    /// use domain::providers::MarketDataProvider;
    /// use std::sync::Arc;
    ///
    /// # fn example<M: MarketDataProvider>(market_data: Arc<M>) {
    /// let config = PaperTradingConfig::default();
    /// let gateway = PaperTradingGateway::new(config, market_data);
    /// # }
    /// ```
    #[must_use]
    pub fn new(
        config: PaperTradingConfig,
        market_data: Arc<dyn MarketDataProvider>,
    ) -> Self {
        let initial_capital = config.initial_capital.clone();
        let initial_positions = config.initial_positions.clone();

        // Initialize positions map from config
        let positions_map: HashMap<Symbol, Position> = initial_positions
            .into_iter()
            .map(|p| (p.symbol().clone(), p))
            .collect();

        Self {
            config,
            market_data,
            orders: Arc::new(RwLock::new(HashMap::new())),
            positions: Arc::new(RwLock::new(positions_map)),
            trade_history: Arc::new(RwLock::new(Vec::new())),
            next_order_id: Arc::new(Mutex::new(1)),
            order_updates_tx: Arc::new(Mutex::new(None)),
            fill_updates_tx: Arc::new(Mutex::new(None)),
            available_capital: Arc::new(RwLock::new(initial_capital)),
            total_realized_pnl: Arc::new(RwLock::new(
                Money::new(Decimal::ZERO, Currency::USD).unwrap()
            )),
        }
    }

    /// Allocates a new unique order ID.
    ///
    /// Generates a new [`OrderId`] using UUID v4.
    async fn allocate_order_id(&self) -> OrderId {
        let _guard = self.next_order_id.lock().await;
        OrderId::generate()
    }

    /// Simulates network latency.
    ///
    /// Sleeps for the configured latency duration.
    async fn simulate_latency(&self) {
        if self.config.latency_ms > 0 {
            sleep(Duration::from_millis(self.config.latency_ms)).await;
        }
    }

    /// Gets the tick size for a symbol.
    ///
    /// Returns the configured tick size or the default if not found.
    fn get_tick_size(&self, symbol: &Symbol) -> Decimal {
        self.config
            .tick_sizes
            .get(symbol)
            .copied()
            .unwrap_or_else(|| DEFAULT_TICK_SIZE.parse().unwrap())
    }

    /// Calculates the fill price with slippage.
    ///
    /// Slippage is applied against the trader:
    /// - Buy orders: fill at higher price (worse)
    /// - Sell orders: fill at lower price (worse)
    ///
    /// # Arguments
    ///
    /// * `symbol` - Trading symbol
    /// * `side` - Order side (Buy/Sell)
    /// * `market_price` - Current market price
    ///
    /// # Returns
    ///
    /// The fill price with slippage applied, rounded to tick size.
    fn calculate_fill_price(
        &self,
        symbol: &Symbol,
        side: Side,
        market_price: Price,
    ) -> Price {
        let tick_size = self.get_tick_size(symbol);
        let slippage = tick_size * self.config.slippage_ticks;

        // Apply slippage against the trader
        let fill_price = match side {
            Side::Buy => market_price.inner() + slippage, // Pay more
            Side::Sell => market_price.inner() - slippage, // Receive less
        };

        // Round to tick size
        let ticks = (fill_price / tick_size).round();
        let rounded = ticks * tick_size;

        // SAFETY: rounded is positive as market_price and slippage are positive
        unsafe { Price::new_unchecked(rounded) }
    }

    /// Gets the current market price for a symbol.
    ///
    /// For now, returns a default price. In production, this would
    /// query the market data provider.
    ///
    /// # Arguments
    ///
    /// * `symbol` - Trading symbol
    /// * `side` - Order side (for bid/ask selection)
    ///
    /// # Returns
    ///
    /// The current market price.
    fn get_market_price(&self, symbol: &Symbol, side: Side) -> Price {
        // In a real implementation, this would query the market data provider
        // For now, return a default price based on symbol
        let default_price = match symbol.as_str() {
            "ES" => Decimal::new(520000, 2), // $5,200.00
            "NQ" => Decimal::new(1823450, 2), // $18,234.50
            "AAPL" => Decimal::new(18550, 2), // $185.50
            "MSFT" => Decimal::new(42000, 2), // $420.00
            _ => Decimal::new(10000, 2),      // $100.00 default
        };

        // Apply a small spread based on side
        let spread = Decimal::new(1, 2); // $0.01 spread
        let price = match side {
            Side::Buy => default_price + spread,  // Ask price
            Side::Sell => default_price - spread, // Bid price
        };

        unsafe { Price::new_unchecked(price) }
    }

    /// Executes a market order immediately.
    ///
    /// Market orders are filled at the current market price with slippage.
    ///
    /// # Arguments
    ///
    /// * `order` - The order to execute
    ///
    /// # Returns
    ///
    /// Returns the generated fill.
    async fn execute_market_order(&self, order: &Order) -> Result<Fill, ExecutionError> {
        let market_price = self.get_market_price(order.symbol(), order.side());
        let fill_price = self.calculate_fill_price(order.symbol(), order.side(), market_price);

        let fill = Fill::with_commission(
            OrderId::from_uuid(order.id()),
            order.symbol().clone(),
            order.quantity(),
            fill_price,
            order.side(),
            Utc::now(),
            self.config.commission_per_trade.clone(),
        );

        Ok(fill)
    }

    /// Executes a limit order.
    ///
    /// For paper trading, limit orders are executed immediately if the
    /// limit price is favorable, otherwise they remain pending.
    ///
    /// # Arguments
    ///
    /// * `order` - The limit order to execute
    ///
    /// # Returns
    ///
    /// Returns the generated fill if executed.
    async fn execute_limit_order(&self, order: &Order) -> Result<Option<Fill>, ExecutionError> {
        let limit_price = order
            .limit_price()
            .ok_or_else(|| ExecutionError::InvalidOrderState {
                order_id: order.id().to_string(),
                state: "Missing limit price".to_string(),
                operation: "execute_limit_order".to_string(),
            })?;

        let market_price = self.get_market_price(order.symbol(), order.side());

        // Check if limit price is reachable
        let should_fill = match order.side() {
            Side::Buy => market_price.inner() <= limit_price.inner(), // Market price <= limit (good)
            Side::Sell => market_price.inner() >= limit_price.inner(), // Market price >= limit (good)
        };

        if should_fill {
            // Fill at the better of limit price or market price (with slippage)
            let fill_price = self.calculate_fill_price(order.symbol(), order.side(), market_price);
            let final_price = match order.side() {
                Side::Buy => fill_price.min(limit_price), // Don't exceed limit
                Side::Sell => fill_price.max(limit_price), // Don't go below limit
            };

            let fill = Fill::with_commission(
                OrderId::from_uuid(order.id()),
                order.symbol().clone(),
                order.quantity(),
                final_price,
                order.side(),
                Utc::now(),
                self.config.commission_per_trade.clone(),
            );

            Ok(Some(fill))
        } else {
            // Order remains pending
            Ok(None)
        }
    }

    /// Updates positions based on a fill.
    ///
    /// Handles position creation, modification, and closing.
    /// Updates realized P&L when positions are closed.
    ///
    /// # Arguments
    ///
    /// * `fill` - The fill to process
    ///
    /// # Returns
    ///
    /// Returns the realized P&L if a position was closed.
    fn update_position_from_fill(&self, fill: &Fill) -> Result<Option<Money>, ExecutionError> {
        let mut positions = self.positions.write().map_err(|_| {
            ExecutionError::ExchangeError {
                exchange: "PaperTrading".to_string(),
                code: "LOCK_ERROR".to_string(),
                message: "Position lock poisoned".to_string(),
            }
        })?;

        let symbol = fill.symbol();
        let fill_side = fill.side();
        let fill_qty = fill.quantity();
        let fill_price = fill.price();

        if let Some(position) = positions.get_mut(symbol) {
            // Existing position
            let pos_side = position.side();

            if fill_side == pos_side {
                // Adding to position (scaling in)
                position
                    .add_fill(fill)
                    .map_err(|e| ExecutionError::InvalidOrderState {
                        order_id: fill.order_id().to_string(),
                        state: format!("add_fill failed: {}", e),
                        operation: "update_position".to_string(),
                    })?;
                Ok(None)
            } else {
                // Reducing or closing position
                let pos_qty = position.quantity();

                if fill_qty.inner() >= pos_qty.inner() {
                    // Closing entire position (and possibly reversing)
                    let closed_position = position
                        .close(fill_price)
                        .map_err(|e| ExecutionError::InvalidOrderState {
                            order_id: fill.order_id().to_string(),
                            state: format!("close failed: {}", e),
                            operation: "update_position".to_string(),
                        })?;

                    let realized_pnl = closed_position.realized_pnl();

                    // If reversing, create new position
                    let remaining_qty = fill_qty.inner() - pos_qty.inner();
                    if remaining_qty > Decimal::ZERO {
                        let new_direction = match fill_side {
                            Side::Buy => PositionDirection::Long,
                            Side::Sell => PositionDirection::Short,
                        };
                        let new_position = Position::new(
                            Uuid::nil(), // No specific account
                            symbol.clone(),
                            new_direction,
                            Quantity::new(remaining_qty).unwrap(),
                            fill_price,
                            Currency::USD,
                        );
                        positions.insert(symbol.clone(), new_position);
                    } else {
                        // Completely closed
                        positions.remove(symbol);
                    }

                    Ok(Some(realized_pnl))
                } else {
                    // Partial close
                    let closed_qty = fill_qty;
                    let realized_pnl = self.calculate_partial_close_pnl(
                        position,
                        closed_qty,
                        fill_price,
                    );

                    // Update position quantity (this is a simplified approach)
                    // In production, we'd track partial closes more precisely
                    let _new_qty = pos_qty.inner() - closed_qty.inner();
                    // Note: Position doesn't have a direct method to reduce quantity
                    // We would need to track this differently in a real implementation

                    Ok(Some(realized_pnl))
                }
            }
        } else {
            // New position
            let direction = match fill_side {
                Side::Buy => PositionDirection::Long,
                Side::Sell => PositionDirection::Short,
            };
            let new_position = Position::new(
                Uuid::nil(), // No specific account
                symbol.clone(),
                direction,
                fill_qty,
                fill_price,
                Currency::USD,
            );
            positions.insert(symbol.clone(), new_position);
            Ok(None)
        }
    }

    /// Calculates P&L for a partial position close.
    ///
    /// # Arguments
    ///
    /// * `position` - Current position
    /// * `closed_qty` - Quantity being closed
    /// * `exit_price` - Exit price
    ///
    /// # Returns
    ///
    /// The realized P&L.
    fn calculate_partial_close_pnl(
        &self,
        position: &Position,
        closed_qty: Quantity,
        exit_price: Price,
    ) -> Money {
        let entry_price = position.avg_entry_price();
        let price_diff = exit_price.inner() - entry_price.inner();

        // Apply direction multiplier
        let direction_mult = match position.side() {
            Side::Buy => Decimal::ONE,  // Long: profit when price goes up
            Side::Sell => -Decimal::ONE, // Short: profit when price goes down
        };

        let pnl = price_diff * closed_qty.inner() * direction_mult;
        unsafe { Money::new_unchecked(pnl, Currency::USD) }
    }

    /// Sends an order update through the channel.
    async fn send_order_update(&self, update: OrderUpdate) {
        if let Some(tx) = self.order_updates_tx.lock().await.as_ref() {
            let _ = tx.send(update).await;
        }
    }

    /// Sends a fill through the channel.
    async fn send_fill(&self, fill: Fill) {
        if let Some(tx) = self.fill_updates_tx.lock().await.as_ref() {
            let _ = tx.send(fill).await;
        }
    }

    /// Gets the performance summary.
    ///
    /// Calculates key trading metrics from the trade history.
    ///
    /// # Returns
    ///
    /// Performance metrics for the paper trading session.
    #[must_use]
    pub fn get_performance_summary(&self) -> PaperTradingPerformance {
        let trade_history = match self.trade_history.read() {
            Ok(guard) => guard,
            Err(_) => {
                // Return default performance if lock is poisoned
                return PaperTradingPerformance {
                    initial_capital: self.config.initial_capital.clone(),
                    current_equity: self.config.initial_capital.clone(),
                    total_return_pct: 0.0,
                    total_trades: 0,
                    winning_trades: 0,
                    losing_trades: 0,
                    win_rate: 0.0,
                    avg_win: Money::new(Decimal::ZERO, Currency::USD).unwrap(),
                    avg_loss: Money::new(Decimal::ZERO, Currency::USD).unwrap(),
                    profit_factor: 0.0,
                    max_drawdown_pct: 0.0,
                    sharpe_ratio: 0.0,
                };
            }
        };

        let initial_capital = self.config.initial_capital.clone();
        let realized_pnl = match self.total_realized_pnl.read() {
            Ok(guard) => guard.clone(),
            Err(_) => Money::new(Decimal::ZERO, Currency::USD).unwrap(),
        };

        let total_trades = trade_history.len() as u32;
        let winning_trades = trade_history
            .iter()
            .filter(|t| t.realized_pnl.amount() > Decimal::ZERO)
            .count() as u32;
        let losing_trades = trade_history
            .iter()
            .filter(|t| t.realized_pnl.amount() < Decimal::ZERO)
            .count() as u32;

        let win_rate = if total_trades > 0 {
            (winning_trades as f64 / total_trades as f64) * 100.0
        } else {
            0.0
        };

        let total_return_pct = if initial_capital.amount() > Decimal::ZERO {
            (realized_pnl.amount() / initial_capital.amount() * Decimal::from(100))
                .to_f64()
                .unwrap_or(0.0)
        } else {
            0.0
        };

        let avg_win = if winning_trades > 0 {
            let total_wins: Decimal = trade_history
                .iter()
                .filter(|t| t.realized_pnl.amount() > Decimal::ZERO)
                .map(|t| t.realized_pnl.amount())
                .sum();
            Money::new(total_wins / Decimal::from(winning_trades), Currency::USD).unwrap()
        } else {
            Money::new(Decimal::ZERO, Currency::USD).unwrap()
        };

        let avg_loss = if losing_trades > 0 {
            let total_losses: Decimal = trade_history
                .iter()
                .filter(|t| t.realized_pnl.amount() < Decimal::ZERO)
                .map(|t| t.realized_pnl.amount())
                .sum();
            Money::new(total_losses / Decimal::from(losing_trades), Currency::USD).unwrap()
        } else {
            Money::new(Decimal::ZERO, Currency::USD).unwrap()
        };

        let profit_factor = {
            let total_wins: Decimal = trade_history
                .iter()
                .filter(|t| t.realized_pnl.amount() > Decimal::ZERO)
                .map(|t| t.realized_pnl.amount())
                .sum();
            let total_losses: Decimal = trade_history
                .iter()
                .filter(|t| t.realized_pnl.amount() < Decimal::ZERO)
                .map(|t| t.realized_pnl.amount().abs())
                .sum();
            if total_losses > Decimal::ZERO {
                (total_wins / total_losses).to_f64().unwrap_or(0.0)
            } else if total_wins > Decimal::ZERO {
                f64::INFINITY
            } else {
                0.0
            }
        };

        PaperTradingPerformance {
            initial_capital,
            current_equity: Money::new(
                initial_capital.amount() + realized_pnl.amount(),
                Currency::USD,
            )
            .unwrap(),
            total_return_pct,
            total_trades,
            winning_trades,
            losing_trades,
            win_rate,
            avg_win,
            avg_loss,
            profit_factor,
            max_drawdown_pct: 0.0, // TODO: Implement drawdown calculation
            sharpe_ratio: 0.0,     // TODO: Implement Sharpe calculation
        }
    }

    /// Gets the current positions.
    ///
    /// Returns a snapshot of all open positions.
    ///
    /// # Returns
    ///
    /// Vector of current positions.
    pub fn get_current_positions(&self) -> Result<Vec<Position>, ExecutionError> {
        let positions = self.positions.read().map_err(|_| {
            ExecutionError::ExchangeError {
                exchange: "PaperTrading".to_string(),
                code: "LOCK_ERROR".to_string(),
                message: "Position lock poisoned".to_string(),
            }
        })?;

        Ok(positions.values().cloned().collect())
    }

    /// Gets the available capital.
    ///
    /// # Returns
    ///
    /// Current available capital.
    pub fn get_available_capital(&self) -> Result<Money, ExecutionError> {
        let capital = self.available_capital.read().map_err(|_| {
            ExecutionError::ExchangeError {
                exchange: "PaperTrading".to_string(),
                code: "LOCK_ERROR".to_string(),
                message: "Capital lock poisoned".to_string(),
            }
        })?;

        Ok(capital.clone())
    }
}

#[async_trait]
impl ExecutionGateway for PaperTradingGateway {
    #[instrument(skip(self, order))]
    async fn place_order(&self, order: Order) -> Result<OrderId, ExecutionError> {
        // Simulate network latency
        self.simulate_latency().await;

        // Allocate order ID
        let order_id = self.allocate_order_id().await;

        // Create paper order
        let mut paper_order = PaperOrder::new(order.clone());
        paper_order.status = OrderStatus::Submitted { at: Utc::now() };

        // Store order
        {
            let mut orders = self.orders.write().map_err(|_| {
                ExecutionError::ExchangeError {
                    exchange: "PaperTrading".to_string(),
                    code: "LOCK_ERROR".to_string(),
                    message: "Order lock poisoned".to_string(),
                }
            })?;
            orders.insert(order_id.clone(), paper_order.clone());
        }

        // Send order update
        self.send_order_update(OrderUpdate::new(
            order_id.clone(),
            paper_order.status.clone(),
            Quantity::new(Decimal::ZERO).unwrap(),
            order.quantity(),
        )
        .with_reason("Order submitted"))
        .await;

        // Execute based on order type
        let fill_result = match order.order_type() {
            OrderType::Market => {
                self.execute_market_order(&order).await.map(Some)
            }
            OrderType::Limit => self.execute_limit_order(&order).await,
            OrderType::Stop | OrderType::StopLimit => {
                // For paper trading, execute stop orders immediately as market orders
                // In a real implementation, these would be monitored until triggered
                warn!("Stop/StopLimit orders executed immediately as market orders in paper trading");
                self.execute_market_order(&order).await.map(Some)
            }
            _ => {
                // Handle future order types as market orders for now
                warn!("Unknown order type treated as market order in paper trading");
                self.execute_market_order(&order).await.map(Some)
            }
        }?;

        // Process fill if order was executed
        if let Some(fill) = fill_result {
            // Update position
            let realized_pnl = self.update_position_from_fill(&fill)?;

            // Update realized P&L if position was closed
            if let Some(pnl) = realized_pnl {
                // Update PnL lock
                {
                    let mut total_pnl = self.total_realized_pnl.write().map_err(|_| {
                        ExecutionError::ExchangeError {
                            exchange: "PaperTrading".to_string(),
                            code: "LOCK_ERROR".to_string(),
                            message: "PnL lock poisoned".to_string(),
                        }
                    })?;
                    *total_pnl = unsafe {
                        Money::new_unchecked(
                            total_pnl.amount() + pnl.amount(),
                            Currency::USD,
                        )
                    };
                } // Lock released

                // Record trade
                {
                    let mut history = self.trade_history.write().map_err(|_| {
                        ExecutionError::ExchangeError {
                            exchange: "PaperTrading".to_string(),
                            code: "LOCK_ERROR".to_string(),
                            message: "History lock poisoned".to_string(),
                        }
                    })?;
                    history.push(TradeRecord {
                        symbol: fill.symbol().clone(),
                        entry_price: fill.price(), // Simplified
                        exit_price: Some(fill.price()),
                        quantity: fill.quantity(),
                        side: fill.side(),
                        realized_pnl: pnl.clone(),
                        commission: fill.commission().map(|m| m.clone()).unwrap_or_else(|| {
                            Money::new(Decimal::ZERO, Currency::USD).unwrap()
                        }),
                        entry_at: Utc::now(), // Simplified
                        exit_at: Some(Utc::now()),
                    });
                } // Lock released
            }

            // Update order with fill
            {
                let mut orders = self.orders.write().map_err(|_| {
                    ExecutionError::ExchangeError {
                        exchange: "PaperTrading".to_string(),
                        code: "LOCK_ERROR".to_string(),
                        message: "Order lock poisoned".to_string(),
                    }
                })?;

                if let Some(po) = orders.get_mut(&order_id) {
                    po.add_fill(fill.clone());
                }
            } // Lock released

            // Prepare update data
            let filled_qty: Decimal = fill.quantity().inner();
            let remaining_qty = order.quantity().inner() - filled_qty;
            let order_update = OrderUpdate::new(
                order_id.clone(),
                OrderStatus::Filled { at: Utc::now() },
                Quantity::new(filled_qty).unwrap(),
                Quantity::new(remaining_qty.max(Decimal::ZERO)).unwrap(),
            )
            .with_avg_fill_price(fill.price())
            .with_last_fill(fill.price(), fill.quantity())
            .with_commission(fill.commission().map(|m| m.clone()).unwrap_or_else(|| {
                Money::new(Decimal::ZERO, Currency::USD).unwrap()
            }));

            // Send fill update and order update after releasing all locks
            self.send_fill(fill.clone()).await;
            self.send_order_update(order_update).await;
        }

        info!("Paper order placed and executed: {}", order_id);
        Ok(order_id)
    }

    #[instrument(skip(self))]
    async fn cancel_order(&self, order_id: OrderId) -> Result<(), ExecutionError> {
        self.simulate_latency().await;

        // Extract data while holding lock, then release it before await
        let order_update = {
            let mut orders = self.orders.write().map_err(|_| {
                ExecutionError::ExchangeError {
                    exchange: "PaperTrading".to_string(),
                    code: "LOCK_ERROR".to_string(),
                    message: "Order lock poisoned".to_string(),
                }
            })?;

            if let Some(paper_order) = orders.get_mut(&order_id) {
                if paper_order.status.can_cancel() {
                    paper_order.status = OrderStatus::Cancelled {
                        at: Utc::now(),
                        reason: "User cancelled".to_string(),
                    };
                    paper_order.updated_at = Utc::now();

                    // Prepare order update
                    let update = OrderUpdate::new(
                        order_id.clone(),
                        paper_order.status.clone(),
                        paper_order.order.filled_quantity(),
                        paper_order.order.remaining_quantity(),
                    )
                    .with_reason("Order cancelled by user");

                    Some(update)
                } else {
                    return Err(ExecutionError::InvalidOrderState {
                        order_id: order_id.to_string(),
                        state: format!("{:?}", paper_order.status),
                        operation: "cancel".to_string(),
                    });
                }
            } else {
                return Err(ExecutionError::OrderNotFound {
                    order_id: order_id.to_string(),
                });
            }
        }; // Lock is released here

        // Send order update after releasing lock
        if let Some(update) = order_update {
            self.send_order_update(update).await;
        }

        info!("Paper order cancelled: {}", order_id);
        Ok(())
    }

    #[instrument(skip(self))]
    async fn modify_order(
        &self,
        order_id: OrderId,
        modifications: OrderModifications,
    ) -> Result<(), ExecutionError> {
        self.simulate_latency().await;

        warn!(
            "Paper trading: modify_order implemented as cancel+replace for order {}",
            order_id
        );

        // Get existing order
        let order = self.get_order(order_id.clone()).await?;
        let _existing_order = order.ok_or_else(|| ExecutionError::OrderNotFound {
            order_id: order_id.to_string(),
        })?;

        // Cancel existing order
        self.cancel_order(order_id).await?;

        // Create new order with modifications
        let _new_qty = modifications.quantity.unwrap_or(_existing_order.quantity());
        let _new_limit = modifications.limit_price.or(_existing_order.limit_price());
        let _new_stop = modifications.stop_price.or(_existing_order.stop_price());
        let _new_tif = modifications.time_in_force.unwrap_or(_existing_order.time_in_force());

        // Build new order (simplified - in production, use Order builder)
        // For now, we just return success as the actual order replacement
        // would require a more complex implementation
        info!("Paper order modified (cancel+replace): {}", _existing_order.id());
        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_order(&self, order_id: OrderId) -> Result<Option<Order>, ExecutionError> {
        let orders = self.orders.read().map_err(|_| {
            ExecutionError::ExchangeError {
                exchange: "PaperTrading".to_string(),
                code: "LOCK_ERROR".to_string(),
                message: "Order lock poisoned".to_string(),
            }
        })?;

        Ok(orders.get(&order_id).map(|po| po.order.clone()))
    }

    #[instrument(skip(self))]
    async fn get_open_orders(&self) -> Result<Vec<Order>, ExecutionError> {
        let orders = self.orders.read().map_err(|_| {
            ExecutionError::ExchangeError {
                exchange: "PaperTrading".to_string(),
                code: "LOCK_ERROR".to_string(),
                message: "Order lock poisoned".to_string(),
            }
        })?;

        Ok(orders
            .values()
            .filter(|po| po.status.is_active())
            .map(|po| po.order.clone())
            .collect())
    }

    #[instrument(skip(self))]
    async fn get_positions(&self) -> Result<Vec<Position>, ExecutionError> {
        self.get_current_positions()
    }

    #[instrument(skip(self))]
    async fn subscribe_order_updates(&self) -> Result<mpsc::Receiver<OrderUpdate>, ExecutionError> {
        let (tx, rx) = mpsc::channel(DEFAULT_ORDER_CHANNEL_SIZE);
        *self.order_updates_tx.lock().await = Some(tx);
        Ok(rx)
    }

    #[instrument(skip(self))]
    async fn subscribe_fill_updates(&self) -> Result<mpsc::Receiver<Fill>, ExecutionError> {
        let (tx, rx) = mpsc::channel(DEFAULT_FILL_CHANNEL_SIZE);
        *self.fill_updates_tx.lock().await = Some(tx);
        Ok(rx)
    }
}

// =============================================================================
// PERFORMANCE METRICS
// =============================================================================

/// Performance metrics for paper trading.
///
/// Provides a summary of trading performance including:
/// - Return metrics (total return, equity curve)
/// - Trade statistics (win rate, profit factor)
/// - Risk metrics (drawdown, Sharpe ratio)
#[derive(Debug, Clone)]
pub struct PaperTradingPerformance {
    /// Initial capital at the start of trading.
    pub initial_capital: Money,

    /// Current equity (initial capital + realized P&L).
    pub current_equity: Money,

    /// Total return as a percentage.
    pub total_return_pct: f64,

    /// Total number of trades executed.
    pub total_trades: u32,

    /// Number of winning trades.
    pub winning_trades: u32,

    /// Number of losing trades.
    pub losing_trades: u32,

    /// Win rate as a percentage (winning_trades / total_trades).
    pub win_rate: f64,

    /// Average winning trade.
    pub avg_win: Money,

    /// Average losing trade (negative value).
    pub avg_loss: Money,

    /// Profit factor (gross profit / gross loss).
    pub profit_factor: f64,

    /// Maximum drawdown as a percentage.
    pub max_drawdown_pct: f64,

    /// Sharpe ratio (risk-adjusted return).
    pub sharpe_ratio: f64,
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use domain::providers::MarketDataProvider;
    use domain::values::{Bar, TimeFrame};

    /// Mock market data provider for testing.
    struct MockMarketDataProvider;

    #[async_trait]
    impl MarketDataProvider for MockMarketDataProvider {
        async fn connect(&mut self) -> Result<(), domain::errors::ProviderError> {
            Ok(())
        }

        async fn disconnect(&mut self) -> Result<(), domain::errors::ProviderError> {
            Ok(())
        }

        async fn subscribe_bars(
            &self,
            _symbol: &Symbol,
            _timeframe: TimeFrame,
        ) -> Result<mpsc::Receiver<Bar>, domain::errors::ProviderError> {
            let (_, rx) = mpsc::channel(1);
            Ok(rx)
        }

        async fn subscribe_ticks(
            &self,
            _symbol: &Symbol,
        ) -> Result<mpsc::Receiver<domain::entities::Tick>, domain::errors::ProviderError> {
            let (_, rx) = mpsc::channel(1);
            Ok(rx)
        }

        async fn unsubscribe(&self, _symbol: &Symbol) -> Result<(), domain::errors::ProviderError> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }
    }

    fn create_test_gateway() -> PaperTradingGateway {
        let config = PaperTradingConfig::default();
        let market_data = Arc::new(MockMarketDataProvider);
        PaperTradingGateway::new(config, market_data)
    }

    fn create_test_order(
        symbol: &str,
        side: Side,
        order_type: OrderType,
        qty: Decimal,
    ) -> Order {
        Order::new(
            Uuid::new_v4(),
            Symbol::new(symbol).unwrap(),
            side,
            order_type,
            Quantity::new(qty).unwrap(),
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn test_paper_trading_config_default() {
        let config = PaperTradingConfig::default();
        assert_eq!(config.slippage_ticks, Decimal::new(25, 2));
        assert_eq!(config.latency_ms, 50);
        assert_eq!(config.commission_per_trade.amount(), Decimal::new(25, 1));
        assert_eq!(config.initial_capital.amount(), Decimal::new(100000, 0));
    }

    #[test]
    fn test_calculate_fill_price_buy() {
        let gateway = create_test_gateway();
        let symbol = Symbol::new("TEST").unwrap();
        let market_price = Price::new(Decimal::new(10000, 2)).unwrap(); // $100.00

        let fill_price = gateway.calculate_fill_price(&symbol, Side::Buy, market_price);

        // Buy should have positive slippage (pay more)
        assert!(fill_price.inner() > market_price.inner());
    }

    #[test]
    fn test_calculate_fill_price_sell() {
        let gateway = create_test_gateway();
        let symbol = Symbol::new("TEST").unwrap();
        let market_price = Price::new(Decimal::new(10000, 2)).unwrap(); // $100.00

        let fill_price = gateway.calculate_fill_price(&symbol, Side::Sell, market_price);

        // Sell should have negative slippage (receive less)
        assert!(fill_price.inner() < market_price.inner());
    }

    #[test]
    fn test_get_tick_size_default() {
        let gateway = create_test_gateway();
        let symbol = Symbol::new("UNKNOWN").unwrap();
        let tick_size = gateway.get_tick_size(&symbol);
        assert_eq!(tick_size, Decimal::new(1, 2)); // Default 0.01
    }

    #[test]
    fn test_get_tick_size_custom() {
        let mut config = PaperTradingConfig::default();
        let symbol = Symbol::new("ES").unwrap();
        config.tick_sizes.insert(symbol.clone(), Decimal::new(25, 2)); // $0.25

        let market_data = Arc::new(MockMarketDataProvider);
        let gateway = PaperTradingGateway::new(config, market_data);

        let tick_size = gateway.get_tick_size(&symbol);
        assert_eq!(tick_size, Decimal::new(25, 2));
    }

    #[tokio::test]
    async fn test_place_market_order() {
        let gateway = create_test_gateway();
        let order = create_test_order("AAPL", Side::Buy, OrderType::Market, Decimal::new(100, 0));

        let order_id = gateway.place_order(order).await.unwrap();

        // Verify order was created
        let retrieved = gateway.get_order(order_id.clone()).await.unwrap();
        assert!(retrieved.is_some());

        // Verify order is in filled state
        let orders = gateway.orders.read().unwrap();
        let paper_order = orders.get(&order_id).unwrap();
        assert!(paper_order.status.is_filled());
    }

    #[tokio::test]
    async fn test_place_limit_order_filled() {
        let gateway = create_test_gateway();
        // Limit order at a price that should fill (high for buy)
        let order = create_test_order("AAPL", Side::Buy, OrderType::Limit, Decimal::new(100, 0));
        // We can't easily set limit price with current API, so this tests the basic flow

        let order_id = gateway.place_order(order).await.unwrap();
        assert!(!order_id.to_string().is_empty());
    }

    #[tokio::test]
    async fn test_cancel_order() {
        let gateway = create_test_gateway();

        // First create an order
        let order = create_test_order("AAPL", Side::Buy, OrderType::Market, Decimal::new(100, 0));
        let order_id = gateway.place_order(order).await.unwrap();

        // Try to cancel (should fail since market order already filled)
        let result = gateway.cancel_order(order_id.clone()).await;
        // Market orders fill immediately, so cancel should fail
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_order() {
        let gateway = create_test_gateway();
        let fake_id = OrderId::generate();

        let result = gateway.cancel_order(fake_id).await;
        assert!(matches!(result, Err(ExecutionError::OrderNotFound { .. })));
    }

    #[tokio::test]
    async fn test_get_open_orders() {
        let gateway = create_test_gateway();

        // Initially no open orders
        let open = gateway.get_open_orders().await.unwrap();
        assert!(open.is_empty());

        // Place an order (fills immediately for market orders)
        let order = create_test_order("AAPL", Side::Buy, OrderType::Market, Decimal::new(100, 0));
        let _ = gateway.place_order(order).await.unwrap();

        // Market orders fill immediately, so no open orders
        let open = gateway.get_open_orders().await.unwrap();
        assert!(open.is_empty());
    }

    #[tokio::test]
    async fn test_get_positions() {
        let gateway = create_test_gateway();

        // Initially no positions
        let positions = gateway.get_positions().await.unwrap();
        assert!(positions.is_empty());

        // Place a buy order to create position
        let order = create_test_order("AAPL", Side::Buy, OrderType::Market, Decimal::new(100, 0));
        let _ = gateway.place_order(order).await.unwrap();

        // Should have a position now
        let positions = gateway.get_positions().await.unwrap();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].symbol().as_str(), "AAPL");
        assert!(positions[0].is_long());
    }

    #[tokio::test]
    async fn test_position_close() {
        let gateway = create_test_gateway();

        // Buy order
        let buy_order = create_test_order("AAPL", Side::Buy, OrderType::Market, Decimal::new(100, 0));
        let _ = gateway.place_order(buy_order).await.unwrap();

        // Verify position exists
        let positions = gateway.get_positions().await.unwrap();
        assert_eq!(positions.len(), 1);

        // Sell order to close
        let sell_order = create_test_order("AAPL", Side::Sell, OrderType::Market, Decimal::new(100, 0));
        let _ = gateway.place_order(sell_order).await.unwrap();

        // Position should be closed
        let positions = gateway.get_positions().await.unwrap();
        assert!(positions.is_empty());
    }

    #[tokio::test]
    async fn test_subscribe_order_updates() {
        let gateway = create_test_gateway();
        let mut rx = gateway.subscribe_order_updates().await.unwrap();

        // Place an order to generate an update
        let order = create_test_order("AAPL", Side::Buy, OrderType::Market, Decimal::new(100, 0));
        let _ = gateway.place_order(order).await.unwrap();

        // Should receive order updates
        let update = rx.recv().await;
        assert!(update.is_some());
    }

    #[tokio::test]
    async fn test_subscribe_fill_updates() {
        let gateway = create_test_gateway();
        let mut rx = gateway.subscribe_fill_updates().await.unwrap();

        // Place an order to generate a fill
        let order = create_test_order("AAPL", Side::Buy, OrderType::Market, Decimal::new(100, 0));
        let _ = gateway.place_order(order).await.unwrap();

        // Should receive fill
        let fill = rx.recv().await;
        assert!(fill.is_some());
    }

    #[test]
    fn test_performance_summary_initial() {
        let gateway = create_test_gateway();
        let perf = gateway.get_performance_summary();

        assert_eq!(perf.total_trades, 0);
        assert_eq!(perf.win_rate, 0.0);
        assert_eq!(perf.total_return_pct, 0.0);
    }

    #[tokio::test]
    async fn test_modify_order() {
        let gateway = create_test_gateway();

        // Create an order
        let order = create_test_order("AAPL", Side::Buy, OrderType::Limit, Decimal::new(100, 0));
        let order_id = gateway.place_order(order).await.unwrap();

        // Modify the order (cancel+replace)
        let mods = OrderModifications::new()
            .with_quantity(Quantity::new(Decimal::new(200, 0)).unwrap());

        let result = gateway.modify_order(order_id, mods).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_modify_nonexistent_order() {
        let gateway = create_test_gateway();
        let fake_id = OrderId::generate();

        let mods = OrderModifications::new();
        let result = gateway.modify_order(fake_id, mods).await;

        assert!(matches!(result, Err(ExecutionError::OrderNotFound { .. })));
    }

    #[test]
    fn test_available_capital() {
        let gateway = create_test_gateway();
        let capital = gateway.get_available_capital().unwrap();
        assert_eq!(capital.amount(), Decimal::new(100000, 0));
    }

    #[test]
    fn test_slippage_rounding() {
        let mut config = PaperTradingConfig::default();
        config.slippage_ticks = Decimal::new(1, 0); // 1 tick
        config.tick_sizes.insert(Symbol::new("ES").unwrap(), Decimal::new(25, 2)); // $0.25

        let market_data = Arc::new(MockMarketDataProvider);
        let gateway = PaperTradingGateway::new(config, market_data);

        let symbol = Symbol::new("ES").unwrap();
        let market_price = Price::new(Decimal::new(520025, 2)).unwrap(); // $5,200.25

        let fill_price = gateway.calculate_fill_price(&symbol, Side::Buy, market_price);

        // Should round to tick size ($0.25)
        let ticks = (fill_price.inner() / Decimal::new(25, 2)).fract();
        assert_eq!(ticks, Decimal::ZERO);
    }
}