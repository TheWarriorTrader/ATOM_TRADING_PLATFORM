//! # Domain Entities
//!
//! Core entities with identity and lifecycle.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::errors::{DomainError, DomainResult};
use crate::values::{Currency, Money, OrderId, Price, Quantity, Side, Symbol, TimeFrame, TimeInForce, Volume};

/// Unique identifier for entities
pub type EntityId = Uuid;

// =============================================================================
// BACKWARD COMPATIBILITY TYPE ALIASES
// =============================================================================

/// Order side (Buy/Sell) - alias for Side
pub type OrderSide = Side;

/// Order type (Market, Limit, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderType {
    /// Market order
    Market,
    /// Limit order
    Limit,
    /// Stop order
    Stop,
    /// Stop-limit order
    StopLimit,
}

impl OrderType {
    /// Returns the string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Market => "MARKET",
            Self::Limit => "LIMIT",
            Self::Stop => "STOP",
            Self::StopLimit => "STOP_LIMIT",
        }
    }

    /// Returns true if this order type requires a limit price.
    #[must_use]
    pub const fn requires_limit_price(&self) -> bool {
        matches!(self, Self::Limit | Self::StopLimit)
    }

    /// Returns true if this order type requires a stop price.
    #[must_use]
    pub const fn requires_stop_price(&self) -> bool {
        matches!(self, Self::Stop | Self::StopLimit)
    }
}

impl fmt::Display for OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Position direction (Long/Short) - maps to Side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PositionDirection {
    /// Long position
    Long,
    /// Short position
    Short,
}

impl PositionDirection {
    /// Returns the corresponding Side
    #[must_use]
    pub const fn to_side(&self) -> Side {
        match self {
            Self::Long => Side::Buy,
            Self::Short => Side::Sell,
        }
    }
}

impl From<PositionDirection> for Side {
    fn from(dir: PositionDirection) -> Self {
        dir.to_side()
    }
}

// =============================================================================
// BAR ENTITY
// =============================================================================

/// OHLCV Bar (candlestick) entity.
///
/// Represents a single period of market data with open, high, low, close prices
/// and volume. This is the fundamental data structure for technical analysis.
///
/// # Invariants
///
/// - `high >= low`
/// - `high >= open` and `high >= close`
/// - `low <= open` and `low <= close`
/// - `volume` is non-negative
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use domain::entities::Bar;
/// use domain::values::{Price, Symbol, TimeFrame, Volume};
/// use rust_decimal::Decimal;
///
/// let bar = Bar::new(
///     Utc::now(),
///     Symbol::new("NQ").unwrap(),
///     Price::new(Decimal::new(1823450, 2)).unwrap(),
///     Price::new(Decimal::new(1823500, 2)).unwrap(),
///     Price::new(Decimal::new(1823375, 2)).unwrap(),
///     Price::new(Decimal::new(1823475, 2)).unwrap(),
///     Volume::new(1420).unwrap(),
///     TimeFrame::M5,
///     "TestProvider".to_string(),
/// ).unwrap();
///
/// assert!(bar.is_bullish());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    /// Bar timestamp (period start)
    timestamp: DateTime<Utc>,
    /// Trading symbol
    symbol: Symbol,
    /// Opening price
    open: Price,
    /// Highest price during period
    high: Price,
    /// Lowest price during period
    low: Price,
    /// Closing price
    close: Price,
    /// Trading volume
    volume: Volume,
    /// Timeframe of the bar
    timeframe: TimeFrame,
    /// Data provider identifier
    provider: String,
}

impl Bar {
    /// Creates a new Bar with validation.
    ///
    /// # Errors
    ///
    /// Returns `DomainError::Validation` if:
    /// - `high < low`
    /// - `high < open` or `high < close`
    /// - `low > open` or `low > close`
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        timestamp: DateTime<Utc>,
        symbol: Symbol,
        open: Price,
        high: Price,
        low: Price,
        close: Price,
        volume: Volume,
        timeframe: TimeFrame,
        provider: impl Into<String>,
    ) -> DomainResult<Self> {
        // Validate high >= low
        if high.inner() < low.inner() {
            return Err(DomainError::validation(format!(
                "High ({}) must be >= low ({})",
                high.inner(),
                low.inner()
            )));
        }

        // Validate high is the maximum
        if high.inner() < open.inner() || high.inner() < close.inner() {
            return Err(DomainError::validation(
                "High must be >= open and close".to_string(),
            ));
        }

        // Validate low is the minimum
        if low.inner() > open.inner() || low.inner() > close.inner() {
            return Err(DomainError::validation(
                "Low must be <= open and close".to_string(),
            ));
        }

        Ok(Self {
            timestamp,
            symbol,
            open,
            high,
            low,
            close,
            volume,
            timeframe,
            provider: provider.into(),
        })
    }

    /// Returns the bar timestamp
    #[must_use]
    pub const fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Returns the symbol
    #[must_use]
    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    /// Returns the open price
    #[must_use]
    pub const fn open(&self) -> Price {
        self.open
    }

    /// Returns the high price
    #[must_use]
    pub const fn high(&self) -> Price {
        self.high
    }

    /// Returns the low price
    #[must_use]
    pub const fn low(&self) -> Price {
        self.low
    }

    /// Returns the close price
    #[must_use]
    pub const fn close(&self) -> Price {
        self.close
    }

    /// Returns the volume
    #[must_use]
    pub const fn volume(&self) -> Volume {
        self.volume
    }

    /// Returns the timeframe
    #[must_use]
    pub const fn timeframe(&self) -> TimeFrame {
        self.timeframe
    }

    /// Returns the provider
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Returns the price range (high - low)
    #[must_use]
    pub fn range(&self) -> Price {
        // SAFETY: high >= low by construction invariant
        unsafe { Price::new_unchecked(self.high.inner() - self.low.inner()) }
    }

    /// Returns the body size (close - open)
    #[must_use]
    pub fn body(&self) -> Decimal {
        self.close.inner() - self.open.inner()
    }

    /// Returns true if the bar is bullish (close > open)
    #[must_use]
    pub fn is_bullish(&self) -> bool {
        self.close.inner() > self.open.inner()
    }

    /// Returns true if the bar is bearish (close < open)
    #[must_use]
    pub fn is_bearish(&self) -> bool {
        self.close.inner() < self.open.inner()
    }

    /// Returns true if the bar is neutral (close == open)
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.close.inner() == self.open.inner()
    }

    /// Returns the typical price: (high + low + close) / 3
    #[must_use]
    pub fn typical_price(&self) -> Price {
        let tp = (self.high.inner() + self.low.inner() + self.close.inner()) / Decimal::new(3, 0);
        // SAFETY: high, low, close are all positive, so result is positive
        unsafe { Price::new_unchecked(tp) }
    }

    /// Returns the weighted close: (high + low + close * 2) / 4
    #[must_use]
    pub fn weighted_close(&self) -> Price {
        let wc = (self.high.inner() + self.low.inner() + self.close.inner() * Decimal::new(2, 0))
            / Decimal::new(4, 0);
        // SAFETY: high, low, close are all positive, so result is positive
        unsafe { Price::new_unchecked(wc) }
    }
}

impl fmt::Display for Bar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {} {} {} {} {}",
            self.symbol,
            self.timestamp.format("%Y-%m-%d %H:%M"),
            self.open,
            self.high,
            self.low,
            self.close,
            self.volume,
            self.timeframe
        )
    }
}

// =============================================================================
// TICK ENTITY
// =============================================================================

/// Market tick entity (tick-by-tick data).
///
/// Represents a single trade or quote update. This is the most granular
/// market data available.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use domain::entities::Tick;
/// use domain::values::{Price, Side, Symbol, Volume};
/// use rust_decimal::Decimal;
///
/// let tick = Tick::new(
///     Utc::now(),
///     Symbol::new("NQ").unwrap(),
///     Price::new(Decimal::new(1823450, 2)).unwrap(),
///     Volume::new(100).unwrap(),
///     Some(Side::Buy),
///     "TestProvider".to_string(),
/// ).unwrap();
///
/// assert!(tick.is_trade());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tick {
    /// Tick timestamp
    timestamp: DateTime<Utc>,
    /// Trading symbol
    symbol: Symbol,
    /// Trade/quote price
    price: Price,
    /// Trade size or bid/ask size
    size: Volume,
    /// Trade side (None for quotes)
    side: Option<Side>,
    /// Data provider identifier
    provider: String,
}

impl Tick {
    /// Creates a new Tick with validation.
    ///
    /// # Errors
    ///
    /// Returns `DomainError::Validation` if the price is not positive.
    pub fn new(
        timestamp: DateTime<Utc>,
        symbol: Symbol,
        price: Price,
        size: Volume,
        side: Option<Side>,
        provider: impl Into<String>,
    ) -> DomainResult<Self> {
        Ok(Self {
            timestamp,
            symbol,
            price,
            size,
            side,
            provider: provider.into(),
        })
    }

    /// Returns the tick timestamp
    #[must_use]
    pub const fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Returns the symbol
    #[must_use]
    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    /// Returns the price
    #[must_use]
    pub const fn price(&self) -> Price {
        self.price
    }

    /// Returns the size
    #[must_use]
    pub const fn size(&self) -> Volume {
        self.size
    }

    /// Returns the side (if a trade)
    #[must_use]
    pub const fn side(&self) -> Option<Side> {
        self.side
    }

    /// Returns the provider
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Returns the notional value of the tick (price * size)
    #[must_use]
    pub fn notional_value(&self) -> Money {
        // SAFETY: price is positive and size is non-negative, result is non-negative
        let amount = self.price.inner() * Decimal::from(self.size.as_i64());
        unsafe { Money::new_unchecked(amount, Currency::USD) }
    }

    /// Returns true if this tick represents a trade (has side)
    #[must_use]
    pub fn is_trade(&self) -> bool {
        self.side.is_some()
    }

    /// Returns true if this tick represents a quote (no side)
    #[must_use]
    pub fn is_quote(&self) -> bool {
        self.side.is_none()
    }
}

impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.side {
            Some(side) => write!(
                f,
                "{} {} x {} @ {} {}",
                self.symbol,
                self.price,
                self.size,
                self.timestamp.format("%H:%M:%S%.3f"),
                side
            ),
            None => write!(
                f,
                "{} {} x {} @ {} (quote)",
                self.symbol,
                self.price,
                self.size,
                self.timestamp.format("%H:%M:%S%.3f")
            ),
        }
    }
}

// =============================================================================
// ORDER STATUS
// =============================================================================

/// Order lifecycle status.
///
/// Represents the current state of an order in the trading lifecycle.
/// State transitions are validated to ensure order integrity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderStatus {
    /// Order created locally but not yet submitted
    Created,
    /// Order submitted to broker/exchange
    Submitted,
    /// Order accepted and pending execution
    Pending,
    /// Order partially filled
    PartiallyFilled,
    /// Order completely filled
    Filled,
    /// Order cancelled
    Cancelled,
    /// Order rejected by broker/exchange
    Rejected,
}

impl fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created => write!(f, "Created"),
            Self::Submitted => write!(f, "Submitted"),
            Self::Pending => write!(f, "Pending"),
            Self::PartiallyFilled => write!(f, "PartiallyFilled"),
            Self::Filled => write!(f, "Filled"),
            Self::Cancelled => write!(f, "Cancelled"),
            Self::Rejected => write!(f, "Rejected"),
        }
    }
}

// =============================================================================
// ORDER ENTITY (LEGACY COMPATIBLE)
// =============================================================================

/// Trading order entity.
///
/// Represents an order to buy or sell a financial instrument.
/// Orders have a lifecycle managed through status transitions.
///
/// # Invariants
///
/// - Limit orders must have a `limit_price`
/// - Stop orders must have a `stop_price`
/// - `filled_quantity <= quantity`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    /// Unique identifier (legacy: uses Uuid)
    id: EntityId,
    /// Account ID (for backward compatibility)
    account_id: EntityId,
    /// Trading symbol
    symbol: Symbol,
    /// Order side
    side: OrderSide,
    /// Order type
    order_type: OrderType,
    /// Order quantity
    quantity: Quantity,
    /// Filled quantity
    filled_quantity: Quantity,
    /// Limit price (for limit orders)
    limit_price: Option<Price>,
    /// Stop price (for stop orders)
    stop_price: Option<Price>,
    /// Current status
    status: OrderStatus,
    /// Time in force
    time_in_force: TimeInForce,
    /// Created timestamp
    created_at: DateTime<Utc>,
    /// Updated timestamp
    updated_at: DateTime<Utc>,
}

impl Order {
    /// Creates a new Order with validation (legacy compatible API).
    ///
    /// # Errors
    ///
    /// Returns `DomainError::OrderValidation` if validation fails
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        account_id: EntityId,
        symbol: Symbol,
        side: OrderSide,
        order_type: OrderType,
        quantity: Quantity,
        limit_price: Option<Price>,
        stop_price: Option<Price>,
    ) -> DomainResult<Self> {
        // Validate limit orders have a limit price
        if order_type.requires_limit_price() && limit_price.is_none() {
            return Err(DomainError::OrderValidation(
                "Limit orders require a limit price".to_string(),
            ));
        }

        // Validate stop orders have a stop price
        if order_type.requires_stop_price() && stop_price.is_none() {
            return Err(DomainError::OrderValidation(
                "Stop orders require a stop price".to_string(),
            ));
        }

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            account_id,
            symbol,
            side,
            order_type,
            quantity,
            filled_quantity: unsafe { Quantity::new_unchecked(Decimal::ZERO) },
            limit_price,
            stop_price,
            status: OrderStatus::Pending,
            time_in_force: TimeInForce::Day,
            created_at: now,
            updated_at: now,
        })
    }

    /// Creates a new Order with full control (new API).
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_id(
        id: OrderId,
        account_id: EntityId,
        symbol: Symbol,
        side: Side,
        quantity: Quantity,
        order_type: OrderType,
        time_in_force: TimeInForce,
        limit_price: Option<Price>,
        stop_price: Option<Price>,
    ) -> DomainResult<Self> {
        // Validate limit orders have a limit price
        if order_type.requires_limit_price() && limit_price.is_none() {
            return Err(DomainError::OrderValidation(
                "Limit orders require a limit price".to_string(),
            ));
        }

        // Validate stop orders have a stop price
        if order_type.requires_stop_price() && stop_price.is_none() {
            return Err(DomainError::OrderValidation(
                "Stop orders require a stop price".to_string(),
            ));
        }

        let now = Utc::now();
        Ok(Self {
            id: id.as_uuid(),
            account_id,
            symbol,
            side,
            order_type,
            quantity,
            filled_quantity: unsafe { Quantity::new_unchecked(Decimal::ZERO) },
            limit_price,
            stop_price,
            status: OrderStatus::Created,
            time_in_force,
            created_at: now,
            updated_at: now,
        })
    }

    /// Returns the order ID
    #[must_use]
    pub const fn id(&self) -> EntityId {
        self.id
    }

    /// Returns the account ID
    #[must_use]
    pub const fn account_id(&self) -> EntityId {
        self.account_id
    }

    /// Returns the symbol
    #[must_use]
    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    /// Returns the order side
    #[must_use]
    pub const fn side(&self) -> OrderSide {
        self.side
    }

    /// Returns the order type
    #[must_use]
    pub const fn order_type(&self) -> OrderType {
        self.order_type
    }

    /// Returns the order quantity
    #[must_use]
    pub const fn quantity(&self) -> Quantity {
        self.quantity
    }

    /// Returns the filled quantity
    #[must_use]
    pub const fn filled_quantity(&self) -> Quantity {
        self.filled_quantity
    }

    /// Returns the remaining quantity to fill
    #[must_use]
    pub fn remaining_quantity(&self) -> Quantity {
        // SAFETY: filled_quantity <= quantity by construction invariant
        unsafe { Quantity::new_unchecked(self.quantity.inner() - self.filled_quantity.inner()) }
    }

    /// Returns the limit price
    #[must_use]
    pub const fn limit_price(&self) -> Option<Price> {
        self.limit_price
    }

    /// Returns the stop price
    #[must_use]
    pub const fn stop_price(&self) -> Option<Price> {
        self.stop_price
    }

    /// Returns the current status
    #[must_use]
    pub const fn status(&self) -> OrderStatus {
        self.status
    }

    /// Returns the time in force
    #[must_use]
    pub const fn time_in_force(&self) -> TimeInForce {
        self.time_in_force
    }

    /// Returns the creation timestamp
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Returns the last update timestamp
    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Returns true if the order can be cancelled
    #[must_use]
    pub fn can_cancel(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::Created | OrderStatus::Submitted | OrderStatus::Pending | OrderStatus::PartiallyFilled
        )
    }

    /// Returns true if the order is in an active state
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::Created | OrderStatus::Submitted | OrderStatus::Pending | OrderStatus::PartiallyFilled
        )
    }

    /// Updates the order status (legacy method for compatibility).
    ///
    /// # Errors
    ///
    /// Returns `DomainError::InvalidStateTransition` if the transition is invalid
    pub fn update_status(&mut self, new_status: OrderStatus) -> DomainResult<()> {
                // Validate state transitions
                let valid_transition = match (self.status, new_status) {
                    // Legacy transitions (kept for compatibility)
                    (OrderStatus::Pending, OrderStatus::Cancelled)
                    | (OrderStatus::Pending, OrderStatus::Rejected)
                    | (OrderStatus::Pending, OrderStatus::PartiallyFilled)
                    | (OrderStatus::Pending, OrderStatus::Filled)
                    | (OrderStatus::PartiallyFilled, OrderStatus::Filled)
                    | (OrderStatus::PartiallyFilled, OrderStatus::Cancelled)
                    | (OrderStatus::Filled, OrderStatus::Filled)
                    | (OrderStatus::Cancelled, OrderStatus::Cancelled)
                    | (OrderStatus::Rejected, OrderStatus::Rejected)
                    // New API transitions
                    | (OrderStatus::Created, OrderStatus::Submitted)
                    | (OrderStatus::Created, OrderStatus::Cancelled)
                    | (OrderStatus::Created, OrderStatus::Rejected)
                    | (OrderStatus::Submitted, OrderStatus::Pending)
                    | (OrderStatus::Submitted, OrderStatus::Rejected)
                    | (OrderStatus::Submitted, OrderStatus::Cancelled)
                    | (OrderStatus::PartiallyFilled, OrderStatus::Rejected) => true,
                    (from, to) if from == to => true,
                    _ => false,
                };
        if !valid_transition {
            return Err(DomainError::InvalidStateTransition {
                from: self.status.to_string(),
                to: new_status.to_string(),
            });
        }

        self.status = new_status;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Fills a portion of the order
    ///
    /// # Errors
    ///
    /// Returns `DomainError::InvalidQuantity` if fill quantity exceeds remaining
    pub fn fill(&mut self, fill_qty: Quantity) -> DomainResult<()> {
        if fill_qty.inner() > self.remaining_quantity().inner() {
            return Err(DomainError::invalid_quantity(
                "Fill quantity exceeds remaining quantity",
            ));
        }

        self.filled_quantity =
            unsafe { Quantity::new_unchecked(self.filled_quantity.inner() + fill_qty.inner()) };

        if self.filled_quantity.inner() == self.quantity.inner() {
            self.status = OrderStatus::Filled;
        } else {
            self.status = OrderStatus::PartiallyFilled;
        }

        self.updated_at = Utc::now();
        Ok(())
    }

    /// Returns the notional value at a given price
    #[must_use]
    pub fn notional_value(&self, current_price: Price) -> Money {
        let amount = current_price.inner() * self.quantity.inner();
        // SAFETY: price and quantity are positive, result is positive
        unsafe { Money::new_unchecked(amount, Currency::USD) }
    }
}

// =============================================================================
// FILL ENTITY
// =============================================================================

/// Represents a fill (execution) of an order.
///
/// A fill occurs when part or all of an order is executed at a specific price.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    /// Fill timestamp
    timestamp: DateTime<Utc>,
    /// Quantity filled
    quantity: Quantity,
    /// Fill price
    price: Price,
    /// Side of the fill
    side: Side,
}

impl Fill {
    /// Creates a new Fill.
    pub fn new(timestamp: DateTime<Utc>, quantity: Quantity, price: Price, side: Side) -> Self {
        Self {
            timestamp,
            quantity,
            price,
            side,
        }
    }

    /// Returns the fill timestamp
    #[must_use]
    pub const fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Returns the fill quantity
    #[must_use]
    pub const fn quantity(&self) -> Quantity {
        self.quantity
    }

    /// Returns the fill price
    #[must_use]
    pub const fn price(&self) -> Price {
        self.price
    }

    /// Returns the fill side
    #[must_use]
    pub const fn side(&self) -> Side {
        self.side
    }
}

// =============================================================================
// CLOSED POSITION ENTITY
// =============================================================================

/// Represents a closed position with its P&L summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosedPosition {
    /// Original position ID
    position_id: EntityId,
    /// Trading symbol
    symbol: Symbol,
    /// Position side
    side: Side,
    /// Total quantity traded
    quantity: Quantity,
    /// Average entry price
    avg_entry_price: Price,
    /// Average exit price
    avg_exit_price: Price,
    /// Realized P&L
    realized_pnl: Money,
    /// Open timestamp
    opened_at: DateTime<Utc>,
    /// Close timestamp
    closed_at: DateTime<Utc>,
}

impl ClosedPosition {
    /// Returns the original position ID
    #[must_use]
    pub const fn position_id(&self) -> EntityId {
        self.position_id
    }

    /// Returns the symbol
    #[must_use]
    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    /// Returns the side
    #[must_use]
    pub const fn side(&self) -> Side {
        self.side
    }

    /// Returns the quantity
    #[must_use]
    pub const fn quantity(&self) -> Quantity {
        self.quantity
    }

    /// Returns the average entry price
    #[must_use]
    pub const fn avg_entry_price(&self) -> Price {
        self.avg_entry_price
    }

    /// Returns the average exit price
    #[must_use]
    pub const fn avg_exit_price(&self) -> Price {
        self.avg_exit_price
    }

    /// Returns the realized P&L
    #[must_use]
    pub const fn realized_pnl(&self) -> Money {
        self.realized_pnl
    }

    /// Returns the open timestamp
    #[must_use]
    pub const fn opened_at(&self) -> DateTime<Utc> {
        self.opened_at
    }

    /// Returns the close timestamp
    #[must_use]
    pub const fn closed_at(&self) -> DateTime<Utc> {
        self.closed_at
    }
}

// =============================================================================
// POSITION ENTITY (LEGACY COMPATIBLE)
// =============================================================================

/// Trading position entity.
///
/// Represents an open position in a financial instrument.
/// Tracks entry price, current price, and unrealized P&L.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// Unique identifier
    id: EntityId,
    /// Account ID (for backward compatibility)
    account_id: EntityId,
    /// Trading symbol
    symbol: Symbol,
    /// Position side (Buy=Long, Sell=Short)
    side: Side,
    /// Position quantity
    quantity: Quantity,
    /// Average entry price
    avg_entry_price: Price,
    /// Current market price
    current_price: Price,
    /// Unrealized P&L
    unrealized_pnl: Money,
    /// Realized P&L (from partial closes)
    realized_pnl: Money,
    /// Position currency
    currency: Currency,
    /// Opened timestamp
    opened_at: DateTime<Utc>,
    /// Last update timestamp
    updated_at: DateTime<Utc>,
}

impl Position {
    /// Creates a new Position (legacy compatible API).
    #[must_use]
    pub fn new(
        account_id: EntityId,
        symbol: Symbol,
        direction: PositionDirection,
        quantity: Quantity,
        avg_entry_price: Price,
        currency: Currency,
    ) -> Self {
        let now = Utc::now();
        let unrealized_pnl = unsafe { Money::new_unchecked(Decimal::ZERO, currency) };
        let realized_pnl = unsafe { Money::new_unchecked(Decimal::ZERO, currency) };

        Self {
            id: Uuid::new_v4(),
            account_id,
            symbol,
            side: direction.to_side(),
            quantity,
            avg_entry_price,
            current_price: avg_entry_price,
            unrealized_pnl,
            realized_pnl,
            currency,
            opened_at: now,
            updated_at: now,
        }
    }

    /// Creates a new Position with new API.
    pub fn new_with_side(
        symbol: Symbol,
        side: Side,
        quantity: Quantity,
        avg_entry_price: Price,
        currency: Currency,
    ) -> DomainResult<Self> {
        let now = Utc::now();
        let unrealized_pnl = unsafe { Money::new_unchecked(Decimal::ZERO, currency) };
        let realized_pnl = unsafe { Money::new_unchecked(Decimal::ZERO, currency) };

        Ok(Self {
            id: Uuid::new_v4(),
            account_id: Uuid::nil(), // No account in new API
            symbol,
            side,
            quantity,
            avg_entry_price,
            current_price: avg_entry_price,
            unrealized_pnl,
            realized_pnl,
            currency,
            opened_at: now,
            updated_at: now,
        })
    }

    /// Returns the position ID
    #[must_use]
    pub const fn id(&self) -> EntityId {
        self.id
    }

    /// Returns the account ID
    #[must_use]
    pub const fn account_id(&self) -> EntityId {
        self.account_id
    }

    /// Returns the symbol
    #[must_use]
    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    /// Returns the position side
    #[must_use]
    pub const fn side(&self) -> Side {
        self.side
    }

    /// Returns the position direction (for backward compatibility)
    #[must_use]
    pub const fn direction(&self) -> PositionDirection {
        match self.side {
            Side::Buy => PositionDirection::Long,
            Side::Sell => PositionDirection::Short,
        }
    }

    /// Returns the position quantity
    #[must_use]
    pub const fn quantity(&self) -> Quantity {
        self.quantity
    }

    /// Returns the average entry price
    #[must_use]
    pub const fn avg_entry_price(&self) -> Price {
        self.avg_entry_price
    }

    /// Returns the current market price
    #[must_use]
    pub const fn current_price(&self) -> Price {
        self.current_price
    }

    /// Returns the unrealized P&L
    #[must_use]
    pub const fn unrealized_pnl(&self) -> Money {
        self.unrealized_pnl
    }

    /// Returns the realized P&L
    #[must_use]
    pub const fn realized_pnl(&self) -> Money {
        self.realized_pnl
    }

    /// Returns the position currency
    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    /// Returns the opened timestamp
    #[must_use]
    pub const fn opened_at(&self) -> DateTime<Utc> {
        self.opened_at
    }

    /// Returns the last update timestamp
    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Returns true if this is a long position (Buy side)
    #[must_use]
    pub fn is_long(&self) -> bool {
        self.side.is_buy()
    }

    /// Returns true if this is a short position (Sell side)
    #[must_use]
    pub fn is_short(&self) -> bool {
        self.side.is_sell()
    }

    /// Returns the market value of the position (quantity * current_price)
    #[must_use]
    pub fn market_value(&self) -> Money {
        let amount = self.current_price.inner() * self.quantity.inner();
        unsafe { Money::new_unchecked(amount, self.currency) }
    }

    /// Updates the current market price and recalculates unrealized P&L.
    ///
    /// This is the legacy API that also takes currency.
    pub fn update_market_price(&mut self, price: Price, _currency: Currency) {
        self.update_price(price);
    }

    /// Updates the current price and recalculates unrealized P&L.
    ///
    /// P&L calculation: `(current_price - avg_entry_price) * quantity * side.sign()`
    pub fn update_price(&mut self, new_price: Price) {
        self.current_price = new_price;

        let price_diff = new_price.inner() - self.avg_entry_price.inner();
        let pnl = price_diff * self.quantity.inner() * Decimal::from(self.side.sign());
        self.unrealized_pnl = unsafe { Money::new_unchecked(pnl, self.currency) };
        self.updated_at = Utc::now();
    }

    /// Adds a fill to the position, updating average entry price.
    ///
    /// This is used when scaling into a position (adding to an existing position).
    ///
    /// # Errors
    ///
    /// Returns `DomainError::Validation` if fill side doesn't match position side.
    pub fn add_fill(&mut self, fill: &Fill) -> DomainResult<()> {
        if fill.side() != self.side {
            return Err(DomainError::validation(
                "Fill side must match position side for adding to position",
            ));
        }

        // Calculate new average entry price using weighted average
        let current_notional = self.avg_entry_price.inner() * self.quantity.inner();
        let fill_notional = fill.price().inner() * fill.quantity().inner();
        let new_quantity = self.quantity.inner() + fill.quantity().inner();

        // SAFETY: new_quantity > 0 because both quantities are positive
        let new_avg_price = (current_notional + fill_notional) / new_quantity;
        self.quantity = unsafe { Quantity::new_unchecked(new_quantity) };
        self.avg_entry_price = unsafe { Price::new_unchecked(new_avg_price) };
        self.updated_at = Utc::now();

        // Recalculate unrealized P&L
        self.update_price(self.current_price);

        Ok(())
    }

    /// Adds realized P&L (for backward compatibility with partial closes)
    pub fn add_realized_pnl(&mut self, pnl: Money) {
        self.realized_pnl = unsafe {
            Money::new_unchecked(
                self.realized_pnl.amount() + pnl.amount(),
                self.realized_pnl.currency(),
            )
        };
        self.updated_at = Utc::now();
    }

    /// Closes the position at the given exit price.
    ///
    /// Returns a `ClosedPosition` with the realized P&L.
    ///
    /// # Errors
    ///
    /// Returns `DomainError::Validation` if exit price is not positive.
    pub fn close(&mut self, exit_price: Price) -> DomainResult<ClosedPosition> {
        // Calculate realized P&L
        let price_diff = exit_price.inner() - self.avg_entry_price.inner();
        let pnl = price_diff * self.quantity.inner() * Decimal::from(self.side.sign());
        let realized_pnl = unsafe { Money::new_unchecked(pnl, self.currency) };

        // Update realized P&L on position
        self.realized_pnl = unsafe {
            Money::new_unchecked(
                self.realized_pnl.amount() + realized_pnl.amount(),
                self.currency,
            )
        };

        let closed_position = ClosedPosition {
            position_id: self.id,
            symbol: self.symbol.clone(),
            side: self.side,
            quantity: self.quantity,
            avg_entry_price: self.avg_entry_price,
            avg_exit_price: exit_price,
            realized_pnl,
            opened_at: self.opened_at,
            closed_at: Utc::now(),
        };

        // Reset position quantities
        self.quantity = unsafe { Quantity::new_unchecked(Decimal::ZERO) };
        self.unrealized_pnl = unsafe { Money::new_unchecked(Decimal::ZERO, self.currency) };
        self.updated_at = Utc::now();

        Ok(closed_position)
    }
}

// =============================================================================
// TRADING ACCOUNT ENTITY
// =============================================================================

/// Trading account entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    /// Unique identifier
    id: EntityId,
    /// Account name
    name: String,
    /// Account currency
    currency: Currency,
    /// Current balance
    balance: Money,
    /// Created timestamp
    created_at: DateTime<Utc>,
    /// Updated timestamp
    updated_at: DateTime<Utc>,
}

impl Account {
    /// Creates a new account
    ///
    /// # Errors
    ///
    /// Returns `DomainError::Validation` if the name is empty
    pub fn new(
        name: impl Into<String>,
        currency: Currency,
        balance: Decimal,
    ) -> DomainResult<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(DomainError::validation("Account name cannot be empty"));
        }

        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            name,
            currency,
            balance: unsafe { Money::new_unchecked(balance, currency) },
            created_at: now,
            updated_at: now,
        })
    }

    /// Returns the account ID
    #[must_use]
    pub const fn id(&self) -> EntityId {
        self.id
    }

    /// Returns the account name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the account currency
    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    /// Returns the current balance
    #[must_use]
    pub const fn balance(&self) -> Money {
        self.balance
    }

    /// Returns the creation timestamp
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Returns the last update timestamp
    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Updates the account balance
    pub fn update_balance(&mut self, new_balance: Decimal) {
        self.balance = unsafe { Money::new_unchecked(new_balance, self.currency) };
        self.updated_at = Utc::now();
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // =========================================================================
    // BAR TESTS
    // =========================================================================

    fn create_valid_bar() -> Bar {
        Bar::new(
            Utc::now(),
            Symbol::new("NQ").unwrap(),
            Price::new(dec!(18234.50)).unwrap(),
            Price::new(dec!(18235.00)).unwrap(),
            Price::new(dec!(18233.75)).unwrap(),
            Price::new(dec!(18234.75)).unwrap(),
            Volume::new(1420).unwrap(),
            TimeFrame::M5,
            "TestProvider",
        )
        .unwrap()
    }

    #[test]
    fn bar_valid_construction() {
        let bar = create_valid_bar();
        assert_eq!(bar.symbol().as_str(), "NQ");
        assert_eq!(bar.open().inner(), dec!(18234.50));
        assert_eq!(bar.high().inner(), dec!(18235.00));
        assert_eq!(bar.low().inner(), dec!(18233.75));
        assert_eq!(bar.close().inner(), dec!(18234.75));
        assert_eq!(bar.volume().as_i64(), 1420);
        assert_eq!(bar.timeframe(), TimeFrame::M5);
    }

    #[test]
    fn bar_validation_high_less_than_low_fails() {
        let result = Bar::new(
            Utc::now(),
            Symbol::new("NQ").unwrap(),
            Price::new(dec!(18234.50)).unwrap(),
            Price::new(dec!(18233.00)).unwrap(), // high < low
            Price::new(dec!(18233.75)).unwrap(),
            Price::new(dec!(18234.75)).unwrap(),
            Volume::new(1420).unwrap(),
            TimeFrame::M5,
            "TestProvider",
        );
        assert!(result.is_err());
    }

    #[test]
    fn bar_validation_high_less_than_open_fails() {
        let result = Bar::new(
            Utc::now(),
            Symbol::new("NQ").unwrap(),
            Price::new(dec!(18236.00)).unwrap(), // open > high
            Price::new(dec!(18235.00)).unwrap(),
            Price::new(dec!(18233.75)).unwrap(),
            Price::new(dec!(18234.75)).unwrap(),
            Volume::new(1420).unwrap(),
            TimeFrame::M5,
            "TestProvider",
        );
        assert!(result.is_err());
    }

    #[test]
    fn bar_bullish_detection() {
        let bullish = Bar::new(
            Utc::now(),
            Symbol::new("NQ").unwrap(),
            Price::new(dec!(100.0)).unwrap(),
            Price::new(dec!(105.0)).unwrap(),
            Price::new(dec!(95.0)).unwrap(),
            Price::new(dec!(102.0)).unwrap(), // close > open
            Volume::new(100).unwrap(),
            TimeFrame::M1,
            "Test",
        )
        .unwrap();
        assert!(bullish.is_bullish());
        assert!(!bullish.is_bearish());
    }

    #[test]
    fn bar_bearish_detection() {
        let bearish = Bar::new(
            Utc::now(),
            Symbol::new("NQ").unwrap(),
            Price::new(dec!(100.0)).unwrap(),
            Price::new(dec!(105.0)).unwrap(),
            Price::new(dec!(95.0)).unwrap(),
            Price::new(dec!(98.0)).unwrap(), // close < open
            Volume::new(100).unwrap(),
            TimeFrame::M1,
            "Test",
        )
        .unwrap();
        assert!(bearish.is_bearish());
        assert!(!bearish.is_bullish());
    }

    #[test]
    fn bar_neutral_detection() {
        let neutral = Bar::new(
            Utc::now(),
            Symbol::new("NQ").unwrap(),
            Price::new(dec!(100.0)).unwrap(),
            Price::new(dec!(105.0)).unwrap(),
            Price::new(dec!(95.0)).unwrap(),
            Price::new(dec!(100.0)).unwrap(), // close == open
            Volume::new(100).unwrap(),
            TimeFrame::M1,
            "Test",
        )
        .unwrap();
        assert!(neutral.is_neutral());
        assert!(!neutral.is_bullish());
        assert!(!neutral.is_bearish());
    }

    #[test]
    fn bar_range_calculation() {
        let bar = create_valid_bar();
        let range = bar.range();
        assert_eq!(range.inner(), dec!(1.25)); // 18235.00 - 18233.75
    }

    #[test]
    fn bar_body_calculation() {
        let bar = create_valid_bar();
        let body = bar.body();
        assert_eq!(body, dec!(0.25)); // 18234.75 - 18234.50
    }

    #[test]
    fn bar_typical_price() {
        let bar = create_valid_bar();
        let tp = bar.typical_price();
        // (high + low + close) / 3 = (18235.00 + 18233.75 + 18234.75) / 3
        assert_eq!(tp.inner(), dec!(18234.5));
    }

    #[test]
    fn bar_weighted_close() {
        let bar = create_valid_bar();
        let wc = bar.weighted_close();
        // (high + low + close * 2) / 4 = (18235.00 + 18233.75 + 18234.75 * 2) / 4
        // = (18235.00 + 18233.75 + 36469.5) / 4 = 72938.25 / 4 = 18234.5625
        assert_eq!(wc.inner(), dec!(18234.5625));
    }

    #[test]
    fn bar_serde_roundtrip() {
        let bar = create_valid_bar();
        let json = serde_json::to_string(&bar).unwrap();
        let deserialized: Bar = serde_json::from_str(&json).unwrap();
        assert_eq!(bar, deserialized);
    }

    // =========================================================================
    // TICK TESTS
    // =========================================================================

    fn create_valid_tick() -> Tick {
        Tick::new(
            Utc::now(),
            Symbol::new("NQ").unwrap(),
            Price::new(dec!(18234.50)).unwrap(),
            Volume::new(100).unwrap(),
            Some(Side::Buy),
            "TestProvider",
        )
        .unwrap()
    }

    #[test]
    fn tick_valid_construction() {
        let tick = create_valid_tick();
        assert_eq!(tick.symbol().as_str(), "NQ");
        assert_eq!(tick.price().inner(), dec!(18234.50));
        assert_eq!(tick.size().as_i64(), 100);
        assert_eq!(tick.side(), Some(Side::Buy));
        assert!(tick.is_trade());
    }

    #[test]
    fn tick_quote_detection() {
        let quote = Tick::new(
            Utc::now(),
            Symbol::new("NQ").unwrap(),
            Price::new(dec!(18234.50)).unwrap(),
            Volume::new(100).unwrap(),
            None, // No side = quote
            "TestProvider",
        )
        .unwrap();
        assert!(quote.is_quote());
        assert!(!quote.is_trade());
    }

    #[test]
    fn tick_notional_value() {
        let tick = create_valid_tick();
        let notional = tick.notional_value();
        assert_eq!(notional.amount(), dec!(1823450)); // 18234.50 * 100
    }

    #[test]
    fn tick_serde_roundtrip() {
        let tick = create_valid_tick();
        let json = serde_json::to_string(&tick).unwrap();
        let deserialized: Tick = serde_json::from_str(&json).unwrap();
        assert_eq!(tick, deserialized);
    }

    // =========================================================================
    // ORDER TESTS
    // =========================================================================

    fn create_valid_market_order() -> Order {
        Order::new(
            Uuid::new_v4(), // account_id
            Symbol::new("NQ").unwrap(),
            Side::Buy,
            OrderType::Market,
            Quantity::new(dec!(10)).unwrap(),
            None,
            None,
        )
        .unwrap()
    }

    fn create_valid_limit_order() -> Order {
        Order::new(
            Uuid::new_v4(), // account_id
            Symbol::new("NQ").unwrap(),
            Side::Sell,
            OrderType::Limit,
            Quantity::new(dec!(5)).unwrap(),
            Some(Price::new(dec!(18235.00)).unwrap()),
            None,
        )
        .unwrap()
    }

    #[test]
    fn order_valid_market_order() {
        let order = create_valid_market_order();
        assert_eq!(order.order_type(), OrderType::Market);
        assert_eq!(order.status(), OrderStatus::Pending);
        assert!(order.can_cancel());
    }

    #[test]
    fn order_valid_limit_order() {
        let order = create_valid_limit_order();
        assert_eq!(order.order_type(), OrderType::Limit);
        assert_eq!(order.side(), Side::Sell);
        assert_eq!(order.limit_price().unwrap().inner(), dec!(18235.00));
    }

    #[test]
    fn order_limit_requires_limit_price() {
        let result = Order::new(
            Uuid::new_v4(),
            Symbol::new("NQ").unwrap(),
            Side::Buy,
            OrderType::Limit,
            Quantity::new(dec!(10)).unwrap(),
            None, // Missing limit price
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn order_stop_requires_stop_price() {
        let result = Order::new(
            Uuid::new_v4(),
            Symbol::new("NQ").unwrap(),
            Side::Buy,
            OrderType::Stop,
            Quantity::new(dec!(10)).unwrap(),
            None,
            None, // Missing stop price
        );
        assert!(result.is_err());
    }

    #[test]
    fn order_remaining_quantity() {
        let order = create_valid_market_order();
        assert_eq!(order.remaining_quantity().inner(), dec!(10));
        assert_eq!(order.filled_quantity().inner(), dec!(0));
    }

    #[test]
    fn order_fill_transition() {
        let mut order = create_valid_market_order();
        
        let fill_qty = Quantity::new(dec!(5)).unwrap();
        order.fill(fill_qty).unwrap();
        
        assert_eq!(order.status(), OrderStatus::PartiallyFilled);
        assert_eq!(order.filled_quantity().inner(), dec!(5));
        assert_eq!(order.remaining_quantity().inner(), dec!(5));
    }

    #[test]
    fn order_complete_fill_transition() {
        let mut order = create_valid_market_order();
        
        let fill_qty = Quantity::new(dec!(10)).unwrap();
        order.fill(fill_qty).unwrap();
        
        assert_eq!(order.status(), OrderStatus::Filled);
        assert!(!order.is_active());
        assert!(!order.can_cancel());
    }

    #[test]
    fn order_cancel_transition() {
        let mut order = create_valid_market_order();
        order.update_status(OrderStatus::Cancelled).unwrap();
        assert_eq!(order.status(), OrderStatus::Cancelled);
    }

    #[test]
    fn order_cannot_cancel_filled() {
        let mut order = create_valid_market_order();
        let fill_qty = Quantity::new(dec!(10)).unwrap();
        order.fill(fill_qty).unwrap();
        
        assert!(!order.can_cancel());
    }

    #[test]
    fn order_reject_transition() {
        let mut order = create_valid_market_order();
        order.update_status(OrderStatus::Rejected).unwrap();
        assert_eq!(order.status(), OrderStatus::Rejected);
    }

    #[test]
    fn order_invalid_transition() {
        let mut order = create_valid_market_order();
        // Cannot go directly from Pending to Created
        assert!(order.update_status(OrderStatus::Created).is_err());
    }

    #[test]
    fn order_notional_value() {
        let order = create_valid_market_order();
        let price = Price::new(dec!(100.0)).unwrap();
        let notional = order.notional_value(price);
        assert_eq!(notional.amount(), dec!(1000.0)); // 10 * 100.0
    }

    #[test]
    fn order_fill_exceeds_remaining_fails() {
        let mut order = create_valid_market_order();
        
        let fill_qty = Quantity::new(dec!(15)).unwrap(); // Exceeds quantity
        assert!(order.fill(fill_qty).is_err());
    }

    #[test]
    fn order_serde_roundtrip() {
        let order = create_valid_limit_order();
        let json = serde_json::to_string(&order).unwrap();
        let deserialized: Order = serde_json::from_str(&json).unwrap();
        assert_eq!(order.id(), deserialized.id());
        assert_eq!(order.symbol().as_str(), deserialized.symbol().as_str());
        assert_eq!(order.side(), deserialized.side());
    }

    // =========================================================================
    // POSITION TESTS
    // =========================================================================

    fn create_long_position() -> Position {
        Position::new(
            Uuid::new_v4(), // account_id
            Symbol::new("NQ").unwrap(),
            PositionDirection::Long,
            Quantity::new(dec!(10)).unwrap(),
            Price::new(dec!(18234.50)).unwrap(),
            Currency::USD,
        )
    }

    fn create_short_position() -> Position {
        Position::new(
            Uuid::new_v4(), // account_id
            Symbol::new("NQ").unwrap(),
            PositionDirection::Short,
            Quantity::new(dec!(10)).unwrap(),
            Price::new(dec!(18234.50)).unwrap(),
            Currency::USD,
        )
    }

    #[test]
    fn position_valid_long_construction() {
        let position = create_long_position();
        assert_eq!(position.symbol().as_str(), "NQ");
        assert!(position.is_long());
        assert!(!position.is_short());
        assert_eq!(position.quantity().inner(), dec!(10));
        assert_eq!(position.avg_entry_price().inner(), dec!(18234.50));
        assert_eq!(position.direction(), PositionDirection::Long);
    }

    #[test]
    fn position_valid_short_construction() {
        let position = create_short_position();
        assert!(position.is_short());
        assert!(!position.is_long());
        assert_eq!(position.side(), Side::Sell);
        assert_eq!(position.direction(), PositionDirection::Short);
    }

    #[test]
    fn position_market_value() {
        let position = create_long_position();
        let mv = position.market_value();
        assert_eq!(mv.amount(), dec!(182345)); // 10 * 18234.50
    }

    #[test]
    fn position_update_price_long_profitable() {
        let mut position = create_long_position();
        // Price goes up - long position should have positive unrealized P&L
        position.update_price(Price::new(dec!(18300.00)).unwrap());
        
        // (18300 - 18234.50) * 10 = 655
        assert!(position.unrealized_pnl().amount() > dec!(0));
        assert_eq!(position.current_price().inner(), dec!(18300.00));
    }

    #[test]
    fn position_update_price_long_unprofitable() {
        let mut position = create_long_position();
        // Price goes down - long position should have negative unrealized P&L
        position.update_price(Price::new(dec!(18200.00)).unwrap());
        
        // (18200 - 18234.50) * 10 = -345
        assert!(position.unrealized_pnl().amount() < dec!(0));
    }

    #[test]
    fn position_update_price_short_profitable() {
        let mut position = create_short_position();
        // Price goes down - short position should have positive unrealized P&L
        position.update_price(Price::new(dec!(18200.00)).unwrap());
        
        // (18234.50 - 18200) * 10 = 345
        assert!(position.unrealized_pnl().amount() > dec!(0));
    }

    #[test]
    fn position_update_price_short_unprofitable() {
        let mut position = create_short_position();
        // Price goes up - short position should have negative unrealized P&L
        position.update_price(Price::new(dec!(18300.00)).unwrap());
        
        // (18234.50 - 18300) * 10 = -655
        assert!(position.unrealized_pnl().amount() < dec!(0));
    }

    #[test]
    fn position_add_fill_updates_avg_price() {
        let mut position = create_long_position();
        // Add another fill at a higher price
        let fill = Fill::new(
            Utc::now(),
            Quantity::new(dec!(10)).unwrap(),
            Price::new(dec!(18300.00)).unwrap(),
            Side::Buy,
        );
        
        position.add_fill(&fill).unwrap();
        
        // New quantity should be 20
        assert_eq!(position.quantity().inner(), dec!(20));
        
        // New avg price should be (18234.50 * 10 + 18300 * 10) / 20 = 18267.25
        assert_eq!(position.avg_entry_price().inner(), dec!(18267.25));
    }

    #[test]
    fn position_add_fill_wrong_side_fails() {
        let mut position = create_long_position();
        let fill = Fill::new(
            Utc::now(),
            Quantity::new(dec!(5)).unwrap(),
            Price::new(dec!(18300.00)).unwrap(),
            Side::Sell, // Wrong side - should be Buy for long position
        );
        
        assert!(position.add_fill(&fill).is_err());
    }

    #[test]
    fn position_close_long_profitable() {
        let mut position = create_long_position();
        position.update_price(Price::new(dec!(18300.00)).unwrap());
        
        let closed = position.close(Price::new(dec!(18300.00)).unwrap()).unwrap();
        
        assert_eq!(closed.side(), Side::Buy);
        assert_eq!(closed.avg_exit_price().inner(), dec!(18300.00));
        // Realized P&L should be (18300 - 18234.50) * 10 = 655
        assert_eq!(closed.realized_pnl().amount(), dec!(655));
        
        // Position should be reset
        assert_eq!(position.quantity().inner(), dec!(0));
    }

    #[test]
    fn position_close_short_profitable() {
        let mut position = create_short_position();
        position.update_price(Price::new(dec!(18200.00)).unwrap());
        
        let closed = position.close(Price::new(dec!(18200.00)).unwrap()).unwrap();
        
        assert_eq!(closed.side(), Side::Sell);
        // Realized P&L should be (18234.50 - 18200) * 10 = 345
        assert_eq!(closed.realized_pnl().amount(), dec!(345));
    }

    #[test]
    fn position_serde_roundtrip() {
        let position = create_long_position();
        let json = serde_json::to_string(&position).unwrap();
        let deserialized: Position = serde_json::from_str(&json).unwrap();
        assert_eq!(position.id(), deserialized.id());
        assert_eq!(position.symbol().as_str(), deserialized.symbol().as_str());
    }

    // =========================================================================
    // EDGE CASE TESTS
    // =========================================================================

    #[test]
    fn bar_zero_volume() {
        let bar = Bar::new(
            Utc::now(),
            Symbol::new("NQ").unwrap(),
            Price::new(dec!(100.0)).unwrap(),
            Price::new(dec!(105.0)).unwrap(),
            Price::new(dec!(95.0)).unwrap(),
            Price::new(dec!(100.0)).unwrap(),
            Volume::zero(),
            TimeFrame::M1,
            "Test",
        )
        .unwrap();
        assert!(bar.volume().is_zero());
    }

    #[test]
    fn tick_zero_size() {
        let tick = Tick::new(
            Utc::now(),
            Symbol::new("NQ").unwrap(),
            Price::new(dec!(100.0)).unwrap(),
            Volume::zero(),
            Some(Side::Buy),
            "Test",
        )
        .unwrap();
        assert_eq!(tick.notional_value().amount(), dec!(0));
    }

    #[test]
    fn order_state_machine_valid_transitions() {
        let mut order = create_valid_market_order();
        
        // Pending -> PartiallyFilled -> Filled
        assert_eq!(order.status(), OrderStatus::Pending);
        
        let fill_qty = Quantity::new(dec!(5)).unwrap();
        order.fill(fill_qty).unwrap();
        assert_eq!(order.status(), OrderStatus::PartiallyFilled);
        
        let fill_qty = Quantity::new(dec!(5)).unwrap();
        order.fill(fill_qty).unwrap();
        assert_eq!(order.status(), OrderStatus::Filled);
    }

    #[test]
    fn position_pnl_calculation_accuracy() {
        // Test exact decimal precision for P&L calculations
        let position = Position::new(
            Uuid::new_v4(),
            Symbol::new("NQ").unwrap(),
            PositionDirection::Long,
            Quantity::new(dec!(3)).unwrap(),
            Price::new(dec!(100.123)).unwrap(),
            Currency::USD,
        );
        
        let mut position = position;
        position.update_price(Price::new(dec!(100.456)).unwrap());
        
        // Expected P&L: (100.456 - 100.123) * 3 = 0.999
        assert_eq!(position.unrealized_pnl().amount(), dec!(0.999));
    }

    #[test]
    fn position_direction_to_side_mapping() {
        assert_eq!(PositionDirection::Long.to_side(), Side::Buy);
        assert_eq!(PositionDirection::Short.to_side(), Side::Sell);
    }

    #[test]
    fn order_type_requires_prices() {
        assert!(OrderType::Market.requires_limit_price() == false);
        assert!(OrderType::Market.requires_stop_price() == false);
        assert!(OrderType::Limit.requires_limit_price() == true);
        assert!(OrderType::Limit.requires_stop_price() == false);
        assert!(OrderType::Stop.requires_limit_price() == false);
        assert!(OrderType::Stop.requires_stop_price() == true);
        assert!(OrderType::StopLimit.requires_limit_price() == true);
        assert!(OrderType::StopLimit.requires_stop_price() == true);
    }
}
