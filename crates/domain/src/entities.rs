//! # Domain Entities
//!
//! Core entities with identity and lifecycle.

use chrono::{DateTime, Utc};
use rust_decimal::{Decimal, prelude::Zero};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
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

/// Order lifecycle status with rich state data.
///
/// Represents the current state of an order in the trading lifecycle.
/// State transitions are validated to ensure order integrity.
///
/// Each variant carries relevant metadata for audit trail and analytics:
/// - Timestamps for all state changes
/// - Fill quantities and prices for partial executions
/// - Reasons for cancellations and rejections
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use domain::entities::OrderStatus;
/// use domain::values::{Price, Quantity};
/// use rust_decimal::Decimal;
///
/// // Order created
/// let status = OrderStatus::Created;
///
/// // Order submitted with timestamp
/// let submitted = OrderStatus::Submitted { at: Utc::now() };
///
/// // Order partially filled with fill details
/// let partial = OrderStatus::PartiallyFilled {
///     filled: Quantity::new(Decimal::new(50, 0)).unwrap(),
///     remaining: Quantity::new(Decimal::new(50, 0)).unwrap(),
///     avg_price: Price::new(Decimal::new(15025, 2)).unwrap(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderStatus {
    /// Order created locally but not yet submitted
    Created,
    /// Order submitted to broker/exchange
    Submitted {
        /// Timestamp when the order was submitted
        at: DateTime<Utc>,
    },
    /// Order accepted and pending execution
    Pending {
        /// Timestamp when the order became pending
        at: DateTime<Utc>,
    },
    /// Order partially filled with execution details
    PartiallyFilled {
        /// Quantity already filled
        filled: Quantity,
        /// Quantity remaining to fill
        remaining: Quantity,
        /// Average fill price so far
        avg_price: Price,
    },
    /// Order completely filled
    Filled {
        /// Timestamp when the order was fully filled
        at: DateTime<Utc>,
    },
    /// Order cancelled with reason
    Cancelled {
        /// Timestamp when the order was cancelled
        at: DateTime<Utc>,
        /// Reason for cancellation (e.g., "user_request", "time_in_force")
        reason: String,
    },
    /// Order rejected by broker/exchange
    Rejected {
        /// Timestamp when the order was rejected
        at: DateTime<Utc>,
        /// Rejection reason from broker/exchange
        reason: String,
    },
}

impl OrderStatus {
    /// Returns true if this status represents an active order (can still be filled).
    ///
    /// Active statuses are: Created, Submitted, Pending, PartiallyFilled
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use domain::entities::OrderStatus;
    ///
    /// assert!(OrderStatus::Created.is_active());
    /// assert!(OrderStatus::Submitted { at: Utc::now() }.is_active());
    /// assert!(!OrderStatus::Filled { at: Utc::now() }.is_active());
    /// assert!(!OrderStatus::Cancelled { at: Utc::now(), reason: "test".to_string() }.is_active());
    /// ```
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Created | Self::Submitted { .. } | Self::Pending { .. } | Self::PartiallyFilled { .. }
        )
    }

    /// Returns true if this status represents a terminal state (no further changes possible).
    ///
    /// Terminal statuses are: Filled, Cancelled, Rejected
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Filled { .. } | Self::Cancelled { .. } | Self::Rejected { .. })
    }

    /// Returns true if the order can be cancelled in this state.
    #[must_use]
    pub fn can_cancel(&self) -> bool {
        matches!(
            self,
            Self::Created | Self::Submitted { .. } | Self::Pending { .. } | Self::PartiallyFilled { .. }
        )
    }

    /// Returns true if this status represents a filled state (complete or partial).
    #[must_use]
    pub fn is_filled(&self) -> bool {
        matches!(self, Self::Filled { .. } | Self::PartiallyFilled { .. })
    }

    /// Returns the timestamp of this status if available.
    ///
    /// Returns `Some(DateTime<Utc>)` for statuses with timestamps,
    /// `None` for `Created` and `PartiallyFilled`.
    #[must_use]
    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Created => None,
            Self::Submitted { at } => Some(*at),
            Self::Pending { at } => Some(*at),
            Self::PartiallyFilled { .. } => None,
            Self::Filled { at } => Some(*at),
            Self::Cancelled { at, .. } => Some(*at),
            Self::Rejected { at, .. } => Some(*at),
        }
    }

    /// Returns the variant name as a static string.
    #[must_use]
    pub const fn variant_name(&self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Submitted { .. } => "Submitted",
            Self::Pending { .. } => "Pending",
            Self::PartiallyFilled { .. } => "PartiallyFilled",
            Self::Filled { .. } => "Filled",
            Self::Cancelled { .. } => "Cancelled",
            Self::Rejected { .. } => "Rejected",
        }
    }
}

impl fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created => write!(f, "Created"),
            Self::Submitted { at } => write!(f, "Submitted(at={})", at.format("%Y-%m-%d %H:%M:%S%.3f")),
            Self::Pending { at } => write!(f, "Pending(at={})", at.format("%Y-%m-%d %H:%M:%S%.3f")),
            Self::PartiallyFilled { filled, remaining, avg_price } => {
                write!(f, "PartiallyFilled(filled={}, remaining={}, avg_price={})", filled, remaining, avg_price)
            }
            Self::Filled { at } => write!(f, "Filled(at={})", at.format("%Y-%m-%d %H:%M:%S%.3f")),
            Self::Cancelled { at, reason } => write!(f, "Cancelled(at={}, reason={})", at.format("%Y-%m-%d %H:%M:%S%.3f"), reason),
            Self::Rejected { at, reason } => write!(f, "Rejected(at={}, reason={})", at.format("%Y-%m-%d %H:%M:%S%.3f"), reason),
        }
    }
}

/// Error type for invalid order status transitions.
#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderStatusError {
    /// Transition not allowed from current state
    #[error("invalid transition from {from} to {to}")]
    InvalidTransition {
        /// Current status
        from: String,
        /// Target status
        to: String,
    },
    /// Missing required data for transition
    #[error("missing required data: {0}")]
    MissingData(String),
    /// Status already terminal
    #[error("order already in terminal state: {0}")]
    TerminalState(String),
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
            status: OrderStatus::Pending { at: now },
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
    pub fn status(&self) -> OrderStatus {
        self.status.clone()
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
/// Returns true if the order can be cancelled.
///
/// An order can be cancelled if it is in an active state:
/// Created, Submitted, Pending, or PartiallyFilled.
#[must_use]
pub fn can_cancel(&self) -> bool {
    self.status.can_cancel()
}

/// Returns true if the order is in an active state.
///
/// Active orders can still receive fills or be cancelled.
#[must_use]
pub fn is_active(&self) -> bool {
    self.status.is_active()
}

/// Returns true if the order is in a terminal state (Filled, Cancelled, Rejected).
#[must_use]
pub fn is_terminal(&self) -> bool {
    self.status.is_terminal()
}

/// Validates if the order can be submitted.
///
/// Checks:
/// - Symbol is valid (non-empty, proper format)
/// - Quantity is positive
/// - Required prices are present for limit/stop orders
/// - Time in force is valid
///
/// # Errors
///
/// Returns `DomainError::Validation` if validation fails.
pub fn validate_for_submission(&self) -> DomainResult<()> {
    // Validate symbol
    if self.symbol.as_str().is_empty() {
        return Err(DomainError::OrderValidation(
            "Symbol cannot be empty".to_string(),
        ));
    }

    // Validate quantity is positive
    if self.quantity.inner() <= Decimal::ZERO {
        return Err(DomainError::invalid_quantity(
            "Order quantity must be positive",
        ));
    }

    // Validate limit orders have a limit price
    if self.order_type.requires_limit_price() && self.limit_price.is_none() {
        return Err(DomainError::OrderValidation(
            "Limit orders require a limit price".to_string(),
        ));
    }

    // Validate stop orders have a stop price
    if self.order_type.requires_stop_price() && self.stop_price.is_none() {
        return Err(DomainError::OrderValidation(
            "Stop orders require a stop price".to_string(),
        ));
    }

    // Validate order is in a state that allows submission
    if !matches!(self.status, OrderStatus::Created) {
        return Err(DomainError::OrderValidation(format!(
            "Cannot submit order in state: {}",
            self.status.variant_name()
        )));
    }

    Ok(())
}

/// Checks if the order's time in force has expired.
///
/// # Arguments
///
/// * `now` - The current timestamp to check against
///
/// # Returns
///
/// `true` if the order's time in force has expired and the order should be cancelled.
#[must_use]
pub fn time_in_force_expired(&self, now: DateTime<Utc>) -> bool {
    match self.time_in_force {
        TimeInForce::GTC => false, // Good till cancelled - never expires
        TimeInForce::Day => {
            // Day orders expire at market close (assumed 16:00 EST)
            // For simplicity, we check if the order was created on a different day
            let created_date = self.created_at.date_naive();
            let now_date = now.date_naive();
            now_date > created_date
        }
        TimeInForce::IOC => {
            // Immediate or cancel - effectively expires if not immediately filled
            // This should be handled by the execution engine
            false
        }
        TimeInForce::FOK => {
            // Fill or kill - expires if not immediately filled completely
            // This should be handled by the execution engine
            false
        }
    }
}

/// Updates the order status with validation of state transitions.
///
/// # Errors
///
/// Returns `DomainError::InvalidStateTransition` if the transition is invalid.
pub fn update_status(&mut self, new_status: OrderStatus) -> DomainResult<()> {
    // Validate state transitions
    let valid_transition = self.is_valid_transition(&new_status);

    if !valid_transition {
        return Err(DomainError::InvalidStateTransition {
            from: self.status.variant_name().to_string(),
            to: new_status.variant_name().to_string(),
        });
    }

    self.status = new_status;
    self.updated_at = Utc::now();
    Ok(())
}

/// Validates if a transition to the new status is allowed.
fn is_valid_transition(&self, new_status: &OrderStatus) -> bool {
    match (&self.status, new_status) {
        // Same state is always valid (idempotent)
        (from, to) if from.variant_name() == to.variant_name() => true,

        // Created can transition to Submitted, Cancelled, or Rejected
        (OrderStatus::Created, OrderStatus::Submitted { .. }) => true,
        (OrderStatus::Created, OrderStatus::Cancelled { .. }) => true,
        (OrderStatus::Created, OrderStatus::Rejected { .. }) => true,

        // Submitted can transition to Pending, Rejected, or Cancelled
        (OrderStatus::Submitted { .. }, OrderStatus::Pending { .. }) => true,
        (OrderStatus::Submitted { .. }, OrderStatus::Rejected { .. }) => true,
        (OrderStatus::Submitted { .. }, OrderStatus::Cancelled { .. }) => true,

        // Pending can transition to PartiallyFilled, Filled, Rejected, or Cancelled
        (OrderStatus::Pending { .. }, OrderStatus::PartiallyFilled { .. }) => true,
        (OrderStatus::Pending { .. }, OrderStatus::Filled { .. }) => true,
        (OrderStatus::Pending { .. }, OrderStatus::Rejected { .. }) => true,
        (OrderStatus::Pending { .. }, OrderStatus::Cancelled { .. }) => true,

        // PartiallyFilled can transition to Filled, Cancelled, or Rejected
        (OrderStatus::PartiallyFilled { .. }, OrderStatus::Filled { .. }) => true,
        (OrderStatus::PartiallyFilled { .. }, OrderStatus::Cancelled { .. }) => true,
        (OrderStatus::PartiallyFilled { .. }, OrderStatus::Rejected { .. }) => true,
        (OrderStatus::PartiallyFilled { .. }, OrderStatus::PartiallyFilled { .. }) => true,

        // Terminal states cannot transition to anything
        (OrderStatus::Filled { .. }, _) => false,
        (OrderStatus::Cancelled { .. }, _) => false,
        (OrderStatus::Rejected { .. }, _) => false,

        // All other transitions are invalid
        _ => false,
    }
}

/// Fills a portion of the order with the given fill details.
///
/// Updates the filled quantity and order status accordingly.
/// Automatically transitions to `Filled` when complete or `PartiallyFilled`
/// when partial.
///
/// # Arguments
///
/// * `fill_qty` - The quantity filled in this execution
/// * `fill_price` - The price at which the fill occurred
///
/// # Errors
///
/// Returns `DomainError::InvalidQuantity` if fill quantity exceeds remaining.
/// Returns `DomainError::InvalidState` if order is not in a fillable state.
pub fn fill(&mut self, fill_qty: Quantity, fill_price: Price) -> DomainResult<()> {
    // Validate order is in a fillable state
    if !self.status.is_active() {
        return Err(DomainError::OrderValidation(format!(
            "Cannot fill order in state: {}",
            self.status.variant_name()
        )));
    }

    if fill_qty.inner() > self.remaining_quantity().inner() {
        return Err(DomainError::invalid_quantity(
            "Fill quantity exceeds remaining quantity",
        ));
    }

    // Update filled quantity
    self.filled_quantity =
        unsafe { Quantity::new_unchecked(self.filled_quantity.inner() + fill_qty.inner()) };

    // Calculate average fill price and remaining
    let remaining = self.remaining_quantity();
    let avg_price = self.calculate_avg_fill_price(fill_qty, fill_price);

    // Update status based on fill completeness
    self.status = if remaining.inner() == Decimal::ZERO {
        OrderStatus::Filled { at: Utc::now() }
    } else {
        OrderStatus::PartiallyFilled {
            filled: self.filled_quantity,
            remaining,
            avg_price,
        }
    };

    self.updated_at = Utc::now();
    Ok(())
}

/// Calculates the average fill price after a new fill.
fn calculate_avg_fill_price(&self, fill_qty: Quantity, fill_price: Price) -> Price {
    let total_filled = self.filled_quantity.inner();
    let prev_filled = total_filled - fill_qty.inner();

    if prev_filled == Decimal::ZERO {
        // First fill, return the fill price
        fill_price
    } else {
        // Calculate weighted average
        // Need to retrieve previous avg price from status if PartiallyFilled
        let prev_avg = match &self.status {
            OrderStatus::PartiallyFilled { avg_price, .. } => avg_price.inner(),
            _ => fill_price.inner(), // Fallback, shouldn't happen in practice
        };

        let new_avg = (prev_avg * prev_filled + fill_price.inner() * fill_qty.inner())
            / total_filled;
        
        // SAFETY: Prices are positive, weighted average is positive
        unsafe { Price::new_unchecked(new_avg) }
    }
}

/// Cancels the order with a reason.
///
/// # Errors
///
/// Returns `DomainError::OrderValidation` if the order cannot be cancelled.
pub fn cancel(&mut self, reason: String) -> DomainResult<()> {
    if !self.can_cancel() {
        return Err(DomainError::OrderValidation(format!(
            "Cannot cancel order in state: {}",
            self.status.variant_name()
        )));
    }

    self.status = OrderStatus::Cancelled {
        at: Utc::now(),
        reason,
    };
    self.updated_at = Utc::now();
    Ok(())
}

/// Rejects the order with a reason.
///
/// # Errors
///
/// Returns `DomainError::OrderValidation` if the order is already terminal.
pub fn reject(&mut self, reason: String) -> DomainResult<()> {
    if self.status.is_terminal() {
        return Err(DomainError::OrderValidation(format!(
            "Cannot reject order in terminal state: {}",
            self.status.variant_name()
        )));
    }

    self.status = OrderStatus::Rejected {
        at: Utc::now(),
        reason,
    };
    self.updated_at = Utc::now();
    Ok(())
}

/// Returns the notional value at a given price.
///
/// Notional value = quantity * price
///
/// # Arguments
///
/// * `current_price` - The price to use for calculation
///
/// # Returns
///
/// The notional value as [`Money`].
#[must_use]
pub fn notional_value(&self, current_price: Price) -> Money {
    let amount = current_price.inner() * self.quantity.inner();
    // SAFETY: price and quantity are positive, result is positive
    unsafe { Money::new_unchecked(amount, Currency::USD) }
}

/// Returns the average entry price for the order.
///
/// For filled or partially filled orders, returns the average fill price.
/// For unfilled orders, returns the limit price if available, otherwise None.
#[must_use]
pub fn avg_entry_price(&self) -> Option<Price> {
    match &self.status {
        OrderStatus::PartiallyFilled { avg_price, .. } => Some(*avg_price),
        OrderStatus::Filled { .. } => {
            // For filled orders, we would need to track avg price separately
            // For now, return limit price as fallback
            self.limit_price
        }
        _ => self.limit_price,
    }
}
}

// =============================================================================
// FILL ENTITY
// =============================================================================

/// Represents a fill (execution) of an order.
///
/// A fill occurs when part or all of an order is executed at a specific price.
/// Contains complete information for trade reconciliation, P&L calculation,
/// and position aggregation.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use domain::entities::Fill;
/// use domain::values::{OrderId, Symbol, Side, Quantity, Price, Currency, Money};
/// use rust_decimal::Decimal;
///
/// let fill = Fill::new(
///     OrderId::generate(),
///     Symbol::new("AAPL").unwrap(),
///     Quantity::new(Decimal::new(100, 0)).unwrap(),
///     Price::new(Decimal::new(15050, 2)).unwrap(),
///     Side::Buy,
///     Utc::now(),
/// );
///
/// assert_eq!(fill.notional_value().currency(), Currency::USD);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    /// Order ID that this fill belongs to
    order_id: OrderId,
    /// Trading symbol
    symbol: Symbol,
    /// Quantity filled
    quantity: Quantity,
    /// Fill price
    price: Price,
    /// Side of the fill (Buy/Sell)
    side: Side,
    /// Fill timestamp
    timestamp: DateTime<Utc>,
    /// Commission paid for this fill (if any)
    commission: Option<Money>,
}

impl Fill {
    /// Creates a new Fill with the required fields.
    ///
    /// # Arguments
    ///
    /// * `order_id` - The order ID this fill belongs to
    /// * `symbol` - The trading symbol
    /// * `quantity` - The quantity filled
    /// * `price` - The fill price
    /// * `side` - The side (Buy/Sell)
    /// * `timestamp` - When the fill occurred
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use domain::entities::Fill;
    /// use domain::values::{OrderId, Symbol, Side, Quantity, Price};
    /// use rust_decimal::Decimal;
    ///
    /// let fill = Fill::new(
    ///     OrderId::generate(),
    ///     Symbol::new("AAPL").unwrap(),
    ///     Quantity::new(Decimal::new(100, 0)).unwrap(),
    ///     Price::new(Decimal::new(15050, 2)).unwrap(),
    ///     Side::Buy,
    ///     Utc::now(),
    /// );
    /// ```
    pub fn new(
        order_id: OrderId,
        symbol: Symbol,
        quantity: Quantity,
        price: Price,
        side: Side,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            order_id,
            symbol,
            quantity,
            price,
            side,
            timestamp,
            commission: None,
        }
    }

    /// Creates a new Fill with commission.
    ///
    /// # Arguments
    ///
    /// * `order_id` - The order ID this fill belongs to
    /// * `symbol` - The trading symbol
    /// * `quantity` - The quantity filled
    /// * `price` - The fill price
    /// * `side` - The side (Buy/Sell)
    /// * `timestamp` - When the fill occurred
    /// * `commission` - Commission paid for this fill
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use domain::entities::Fill;
    /// use domain::values::{OrderId, Symbol, Side, Quantity, Price, Money, Currency};
    /// use rust_decimal::Decimal;
    ///
    /// let fill = Fill::with_commission(
    ///     OrderId::generate(),
    ///     Symbol::new("AAPL").unwrap(),
    ///     Quantity::new(Decimal::new(100, 0)).unwrap(),
    ///     Price::new(Decimal::new(15050, 2)).unwrap(),
    ///     Side::Buy,
    ///     Utc::now(),
    ///     Money::new(Decimal::new(1, 0), Currency::USD).unwrap(),
    /// );
    /// ```
    pub fn with_commission(
        order_id: OrderId,
        symbol: Symbol,
        quantity: Quantity,
        price: Price,
        side: Side,
        timestamp: DateTime<Utc>,
        commission: Money,
    ) -> Self {
        Self {
            order_id,
            symbol,
            quantity,
            price,
            side,
            timestamp,
            commission: Some(commission),
        }
    }

    /// Returns the order ID
    #[must_use]
    pub const fn order_id(&self) -> OrderId {
        self.order_id
    }

    /// Returns the trading symbol
    #[must_use]
    pub fn symbol(&self) -> &Symbol {
        &self.symbol
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

    /// Returns the fill timestamp
    #[must_use]
    pub const fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Returns the commission (if any)
    #[must_use]
    pub const fn commission(&self) -> Option<Money> {
        self.commission
    }

    /// Returns the notional value of this fill (quantity * price).
    ///
    /// Notional value represents the total value of the trade before commissions.
    ///
    /// # Returns
    ///
    /// The notional value as [`Money`] in USD.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use domain::entities::Fill;
    /// use domain::values::{OrderId, Symbol, Side, Quantity, Price, Currency};
    /// use rust_decimal::Decimal;
    ///
    /// let fill = Fill::new(
    ///     OrderId::generate(),
    ///     Symbol::new("AAPL").unwrap(),
    ///     Quantity::new(Decimal::new(100, 0)).unwrap(),
    ///     Price::new(Decimal::new(15050, 2)).unwrap(),
    ///     Side::Buy,
    ///     Utc::now(),
    /// );
    ///
    /// let notional = fill.notional_value();
    /// assert_eq!(notional.amount(), Decimal::new(15050, 0)); // $15,050.00
    /// assert_eq!(notional.currency(), Currency::USD);
    /// ```
    #[must_use]
    pub fn notional_value(&self) -> Money {
        let amount = self.price.inner() * self.quantity.inner();
        // SAFETY: price and quantity are positive, result is positive
        unsafe { Money::new_unchecked(amount, Currency::USD) }
    }

    /// Returns the net value including commission.
    ///
    /// For buy orders: net_value = notional + commission
    /// For sell orders: net_value = notional - commission
    ///
    /// # Returns
    ///
    /// The net value as [`Money`].
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use domain::entities::Fill;
    /// use domain::values::{OrderId, Symbol, Side, Quantity, Price, Money, Currency};
    /// use rust_decimal::Decimal;
    ///
    /// let fill = Fill::with_commission(
    ///     OrderId::generate(),
    ///     Symbol::new("AAPL").unwrap(),
    ///     Quantity::new(Decimal::new(100, 0)).unwrap(),
    ///     Price::new(Decimal::new(15050, 2)).unwrap(),
    ///     Side::Buy,
    ///     Utc::now(),
    ///     Money::new(Decimal::new(5, 0), Currency::USD).unwrap(),
    /// );
    ///
    /// let net = fill.net_value();
    /// // $15,050 + $5 commission = $15,055
    /// assert_eq!(net.amount(), Decimal::new(15055, 0));
    /// ```
    #[must_use]
    pub fn net_value(&self) -> Money {
        let notional = self.notional_value();
        
        match self.commission {
            Some(comm) => {
                match self.side {
                    Side::Buy => {
                        // Buy: pay more (notional + commission)
                        let total = notional.amount() + comm.amount();
                        unsafe { Money::new_unchecked(total, Currency::USD) }
                    }
                    Side::Sell => {
                        // Sell: receive less (notional - commission)
                        let total = notional.amount() - comm.amount();
                        unsafe { Money::new_unchecked(total, Currency::USD) }
                    }
                }
            }
            None => notional,
        }
    }

    /// Calculates the P&L for this fill given an entry price.
    ///
    /// For long positions (Buy fills):
    /// - P&L = (exit_price - entry_price) * quantity - commission
    ///
    /// For short positions (Sell fills as exit):
    /// - P&L = (entry_price - exit_price) * quantity - commission
    ///
    /// # Arguments
    ///
    /// * `entry_price` - The average entry price of the position
    ///
    /// # Returns
    ///
    /// The realized P&L as [`Money`]. Positive values indicate profit.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use domain::entities::Fill;
    /// use domain::values::{OrderId, Symbol, Side, Quantity, Price, Money, Currency};
    /// use rust_decimal::Decimal;
    ///
    /// // Sell 100 shares at $160 (closing a long position entered at $150)
    /// let exit_fill = Fill::with_commission(
    ///     OrderId::generate(),
    ///     Symbol::new("AAPL").unwrap(),
    ///     Quantity::new(Decimal::new(100, 0)).unwrap(),
    ///     Price::new(Decimal::new(16000, 2)).unwrap(), // $160.00
    ///     Side::Sell,
    ///     Utc::now(),
    ///     Money::new(Decimal::new(5, 0), Currency::USD).unwrap(),
    /// );
    ///
    /// let entry_price = Price::new(Decimal::new(15000, 2)).unwrap(); // $150.00
    /// let pnl = exit_fill.pnl(entry_price);
    ///
    /// // ($160 - $150) * 100 - $5 = $1000 - $5 = $995 profit
    /// assert_eq!(pnl.amount(), Decimal::new(995, 0));
    /// ```
    #[must_use]
    pub fn pnl(&self, entry_price: Price) -> Money {
        let price_diff = match self.side {
            Side::Buy => entry_price.inner() - self.price.inner(),  // Long: entry - current
            Side::Sell => self.price.inner() - entry_price.inner(), // Short: current - entry (for Sell as exit)
        };

        // For proper P&L calculation, we need to know if this is an opening or closing fill
        // This is a simplified version assuming this fill is the closing fill
        let gross_pnl = price_diff * self.quantity.inner();
        
        // Subtract commission
        let commission_amount = self.commission.map(|c| c.amount()).unwrap_or_else(Decimal::zero);
        let net_pnl = gross_pnl - commission_amount;

        unsafe { Money::new_unchecked(net_pnl, Currency::USD) }
    }

    /// Returns true if this fill closes a position (opposite side).
    ///
    /// # Arguments
    ///
    /// * `position_side` - The current side of the position (Long/Short)
    ///
    /// # Returns
    ///
    /// `true` if the fill reduces or closes the position.
    #[must_use]
    pub fn is_closing(&self, position_side: PositionDirection) -> bool {
        match (position_side, self.side) {
            (PositionDirection::Long, Side::Sell) => true,
            (PositionDirection::Short, Side::Buy) => true,
            _ => false,
        }
    }

    /// Returns a builder for creating a fill with additional fields.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use domain::entities::Fill;
    /// use domain::values::{OrderId, Symbol, Side, Quantity, Price, Money, Currency};
    /// use rust_decimal::Decimal;
    ///
    /// let fill = Fill::builder(
    ///     OrderId::generate(),
    ///     Symbol::new("AAPL").unwrap(),
    ///     Quantity::new(Decimal::new(100, 0)).unwrap(),
    ///     Price::new(Decimal::new(15050, 2)).unwrap(),
    ///     Side::Buy,
    ///     Utc::now(),
    /// )
    /// .commission(Money::new(Decimal::new(5, 0), Currency::USD).unwrap())
    /// .build();
    /// ```
    pub fn builder(
        order_id: OrderId,
        symbol: Symbol,
        quantity: Quantity,
        price: Price,
        side: Side,
        timestamp: DateTime<Utc>,
    ) -> FillBuilder {
        FillBuilder::new(order_id, symbol, quantity, price, side, timestamp)
    }
}

/// Builder for constructing Fill instances with optional fields.
#[derive(Debug, Clone)]
pub struct FillBuilder {
    order_id: OrderId,
    symbol: Symbol,
    quantity: Quantity,
    price: Price,
    side: Side,
    timestamp: DateTime<Utc>,
    commission: Option<Money>,
}

impl FillBuilder {
    /// Creates a new FillBuilder with required fields.
    fn new(
        order_id: OrderId,
        symbol: Symbol,
        quantity: Quantity,
        price: Price,
        side: Side,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            order_id,
            symbol,
            quantity,
            price,
            side,
            timestamp,
            commission: None,
        }
    }

    /// Sets the commission for this fill.
    pub fn commission(mut self, commission: Money) -> Self {
        self.commission = Some(commission);
        self
    }

    /// Builds the Fill instance.
    pub fn build(self) -> Fill {
        Fill {
            order_id: self.order_id,
            symbol: self.symbol,
            quantity: self.quantity,
            price: self.price,
            side: self.side,
            timestamp: self.timestamp,
            commission: self.commission,
        }
    }
}

/// Aggregates multiple fills into position metrics.
///
/// This struct provides utilities for calculating aggregate statistics
/// from a collection of fills.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillAggregation {
    /// Total quantity filled
    total_quantity: Quantity,
    /// Average fill price (weighted by quantity)
    avg_price: Price,
    /// Total notional value
    total_notional: Money,
    /// Total commission paid
    total_commission: Money,
    /// Number of fills
    fill_count: usize,
}

impl FillAggregation {
    /// Creates a new empty aggregation.
    pub fn new(currency: Currency) -> Self {
        Self {
            total_quantity: unsafe { Quantity::new_unchecked(Decimal::ZERO) },
            avg_price: unsafe { Price::new_unchecked(Decimal::ZERO) },
            total_notional: unsafe { Money::new_unchecked(Decimal::ZERO, currency) },
            total_commission: unsafe { Money::new_unchecked(Decimal::ZERO, currency) },
            fill_count: 0,
        }
    }

    /// Aggregates fills from an iterator.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use domain::entities::{Fill, FillAggregation};
    /// use domain::values::{OrderId, Symbol, Side, Quantity, Price, Currency};
    /// use rust_decimal::Decimal;
    ///
    /// let fills = vec![
    ///     Fill::new(
    ///         OrderId::generate(),
    ///         Symbol::new("AAPL").unwrap(),
    ///         Quantity::new(Decimal::new(50, 0)).unwrap(),
    ///         Price::new(Decimal::new(15000, 2)).unwrap(),
    ///         Side::Buy,
    ///         Utc::now(),
    ///     ),
    ///     Fill::new(
    ///         OrderId::generate(),
    ///         Symbol::new("AAPL").unwrap(),
    ///         Quantity::new(Decimal::new(50, 0)).unwrap(),
    ///         Price::new(Decimal::new(15100, 2)).unwrap(),
    ///         Side::Buy,
    ///         Utc::now(),
    ///     ),
    /// ];
    ///
    /// let agg = FillAggregation::from_fills(fills.iter(), Currency::USD);
    /// assert_eq!(agg.total_quantity().inner(), Decimal::new(100, 0));
    /// ```
    pub fn from_fills<'a>(fills: impl Iterator<Item = &'a Fill>, currency: Currency) -> Self {
        let mut agg = Self::new(currency);
        for fill in fills {
            agg.add_fill(fill);
        }
        agg
    }

    /// Adds a fill to the aggregation.
    pub fn add_fill(&mut self, fill: &Fill) {
        let fill_qty = fill.quantity().inner();
        let fill_price = fill.price().inner();
        
        // Update total notional
        let notional = fill_price * fill_qty;
        self.total_notional = unsafe {
            Money::new_unchecked(self.total_notional.amount() + notional, self.total_notional.currency())
        };

        // Update total commission
        if let Some(comm) = fill.commission() {
            self.total_commission = unsafe {
                Money::new_unchecked(
                    self.total_commission.amount() + comm.amount(),
                    self.total_commission.currency(),
                )
            };
        }

        // Update weighted average price
        let prev_qty = self.total_quantity.inner();
        self.total_quantity = unsafe {
            Quantity::new_unchecked(prev_qty + fill_qty)
        };

        if self.total_quantity.inner() > Decimal::ZERO {
            let new_avg = if prev_qty == Decimal::ZERO {
                fill_price
            } else {
                let prev_notional = self.avg_price.inner() * prev_qty;
                (prev_notional + notional) / self.total_quantity.inner()
            };
            self.avg_price = unsafe { Price::new_unchecked(new_avg) };
        }

        self.fill_count += 1;
    }

    /// Returns the total quantity filled.
    #[must_use]
    pub const fn total_quantity(&self) -> Quantity {
        self.total_quantity
    }

    /// Returns the average fill price.
    #[must_use]
    pub const fn avg_price(&self) -> Price {
        self.avg_price
    }

    /// Returns the total notional value.
    #[must_use]
    pub const fn total_notional(&self) -> Money {
        self.total_notional
    }

    /// Returns the total commission paid.
    #[must_use]
    pub const fn total_commission(&self) -> Money {
        self.total_commission
    }

    /// Returns the number of fills aggregated.
    #[must_use]
    pub const fn fill_count(&self) -> usize {
        self.fill_count
    }

    /// Returns the net value (notional - commission for buys, notional + commission for sells).
    ///
    /// Note: This assumes all fills are on the same side. Mixed fills should be
    /// handled separately.
    #[must_use]
    pub fn net_value(&self, side: Side) -> Money {
        let total = self.total_notional.amount();
        let commission = self.total_commission.amount();
        
        let net = match side {
            Side::Buy => total + commission,  // Buys cost more with commission
            Side::Sell => total - commission, // Sells yield less with commission
        };
        
        unsafe { Money::new_unchecked(net, self.total_notional.currency()) }
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
        assert!(matches!(order.status(), OrderStatus::Pending { .. }));
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
        let fill_price = Price::new(dec!(150.0)).unwrap();
        order.fill(fill_qty, fill_price).unwrap();
        
        assert!(matches!(order.status(), OrderStatus::PartiallyFilled { .. }));
        assert_eq!(order.filled_quantity().inner(), dec!(5));
        assert_eq!(order.remaining_quantity().inner(), dec!(5));
    }

    #[test]
    fn order_complete_fill_transition() {
        let mut order = create_valid_market_order();
        
        let fill_qty = Quantity::new(dec!(10)).unwrap();
        let fill_price = Price::new(dec!(150.0)).unwrap();
        order.fill(fill_qty, fill_price).unwrap();
        
        assert!(matches!(order.status(), OrderStatus::Filled { .. }));
        assert!(!order.is_active());
        assert!(!order.can_cancel());
    }

    #[test]
    fn order_cancel_transition() {
        let mut order = create_valid_market_order();
        order.cancel("user_request".to_string()).unwrap();
        assert!(matches!(order.status(), OrderStatus::Cancelled { .. }));
    }

    #[test]
    fn order_cannot_cancel_filled() {
        let mut order = create_valid_market_order();
        let fill_qty = Quantity::new(dec!(10)).unwrap();
        let fill_price = Price::new(dec!(150.0)).unwrap();
        order.fill(fill_qty, fill_price).unwrap();
        
        assert!(!order.can_cancel());
    }

    #[test]
    fn order_reject_transition() {
        let mut order = create_valid_market_order();
        order.reject("insufficient_funds".to_string()).unwrap();
        assert!(matches!(order.status(), OrderStatus::Rejected { .. }));
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
        let fill_price = Price::new(dec!(150.0)).unwrap();
        assert!(order.fill(fill_qty, fill_price).is_err());
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
            OrderId::generate(),
            Symbol::new("NQ").unwrap(),
            Quantity::new(dec!(10)).unwrap(),
            Price::new(dec!(18300.00)).unwrap(),
            Side::Buy,
            Utc::now(),
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
            OrderId::generate(),
            Symbol::new("NQ").unwrap(),
            Quantity::new(dec!(5)).unwrap(),
            Price::new(dec!(18300.00)).unwrap(),
            Side::Sell, // Wrong side - should be Buy for long position
            Utc::now(),
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
    // FILL TESTS
    // =========================================================================

    fn create_test_fill(side: Side, quantity: Decimal, price: Decimal) -> Fill {
        Fill::new(
            OrderId::generate(),
            Symbol::new("AAPL").unwrap(),
            Quantity::new(quantity).unwrap(),
            Price::new(price).unwrap(),
            side,
            Utc::now(),
        )
    }

    fn create_test_fill_with_commission(
        side: Side,
        quantity: Decimal,
        price: Decimal,
        commission: Decimal,
    ) -> Fill {
        Fill::with_commission(
            OrderId::generate(),
            Symbol::new("AAPL").unwrap(),
            Quantity::new(quantity).unwrap(),
            Price::new(price).unwrap(),
            side,
            Utc::now(),
            Money::new(commission, Currency::USD).unwrap(),
        )
    }

    #[test]
    fn fill_valid_construction() {
        let fill = create_test_fill(Side::Buy, dec!(100), dec!(150.50));
        
        assert_eq!(fill.symbol().as_str(), "AAPL");
        assert_eq!(fill.quantity().inner(), dec!(100));
        assert_eq!(fill.price().inner(), dec!(150.50));
        assert_eq!(fill.side(), Side::Buy);
        assert!(fill.commission().is_none());
    }

    #[test]
    fn fill_with_commission_construction() {
        let fill = create_test_fill_with_commission(Side::Sell, dec!(50), dec!(200.00), dec!(5.00));
        
        assert_eq!(fill.quantity().inner(), dec!(50));
        assert_eq!(fill.side(), Side::Sell);
        assert!(fill.commission().is_some());
        assert_eq!(fill.commission().unwrap().amount(), dec!(5.00));
        assert_eq!(fill.commission().unwrap().currency(), Currency::USD);
    }

    #[test]
    fn fill_notional_value_calculation() {
        let fill = create_test_fill(Side::Buy, dec!(100), dec!(150.00));
        let notional = fill.notional_value();
        
        // 100 * $150 = $15,000
        assert_eq!(notional.amount(), dec!(15000));
        assert_eq!(notional.currency(), Currency::USD);
    }

    #[test]
    fn fill_notional_value_fractional() {
        let fill = create_test_fill(Side::Buy, dec!(10.5), dec!(150.25));
        let notional = fill.notional_value();
        
        // 10.5 * $150.25 = $1,577.625
        assert_eq!(notional.amount(), dec!(1577.625));
    }

    #[test]
    fn fill_net_value_buy_with_commission() {
        // Buy: net = notional + commission (pay more)
        let fill = create_test_fill_with_commission(Side::Buy, dec!(100), dec!(150.00), dec!(7.50));
        let net = fill.net_value();
        
        // $15,000 + $7.50 = $15,007.50
        assert_eq!(net.amount(), dec!(15007.50));
    }

    #[test]
    fn fill_net_value_sell_with_commission() {
        // Sell: net = notional - commission (receive less)
        let fill = create_test_fill_with_commission(Side::Sell, dec!(100), dec!(160.00), dec!(5.00));
        let net = fill.net_value();
        
        // $16,000 - $5 = $15,995
        assert_eq!(net.amount(), dec!(15995));
    }

    #[test]
    fn fill_net_value_without_commission() {
        let fill = create_test_fill(Side::Buy, dec!(100), dec!(150.00));
        let net = fill.net_value();
        
        // Without commission, net = notional
        assert_eq!(net.amount(), dec!(15000));
    }

    #[test]
    fn fill_pnl_long_position_profit() {
        // Long position: bought at $150, selling at $160
        let exit_fill = create_test_fill_with_commission(Side::Sell, dec!(100), dec!(160.00), dec!(5.00));
        let entry_price = Price::new(dec!(150.00)).unwrap();
        let pnl = exit_fill.pnl(entry_price);
        
        // ($160 - $150) * 100 - $5 = $1,000 - $5 = $995 profit
        assert_eq!(pnl.amount(), dec!(995));
    }

    #[test]
    fn fill_pnl_long_position_loss() {
        // Long position: bought at $150, selling at $140
        let exit_fill = create_test_fill_with_commission(Side::Sell, dec!(100), dec!(140.00), dec!(5.00));
        let entry_price = Price::new(dec!(150.00)).unwrap();
        let pnl = exit_fill.pnl(entry_price);
        
        // ($140 - $150) * 100 - $5 = -$1,000 - $5 = -$1,005 loss
        assert_eq!(pnl.amount(), dec!(-1005));
    }

    #[test]
    fn fill_pnl_zero_profit() {
        // Break-even exit
        let exit_fill = create_test_fill_with_commission(Side::Sell, dec!(100), dec!(150.00), dec!(5.00));
        let entry_price = Price::new(dec!(150.00)).unwrap();
        let pnl = exit_fill.pnl(entry_price);
        
        // ($150 - $150) * 100 - $5 = -$5 (just commission loss)
        assert_eq!(pnl.amount(), dec!(-5));
    }

    #[test]
    fn fill_pnl_without_commission() {
        let exit_fill = create_test_fill(Side::Sell, dec!(100), dec!(160.00));
        let entry_price = Price::new(dec!(150.00)).unwrap();
        let pnl = exit_fill.pnl(entry_price);
        
        // ($160 - $150) * 100 = $1,000 profit (no commission)
        assert_eq!(pnl.amount(), dec!(1000));
    }

    #[test]
    fn fill_is_closing_long_position() {
        let sell_fill = create_test_fill(Side::Sell, dec!(100), dec!(160.00));
        
        // Sell fill closes long position
        assert!(sell_fill.is_closing(PositionDirection::Long));
        // Sell fill does NOT close short position (it would increase it)
        assert!(!sell_fill.is_closing(PositionDirection::Short));
    }

    #[test]
    fn fill_is_closing_short_position() {
        let buy_fill = create_test_fill(Side::Buy, dec!(100), dec!(140.00));
        
        // Buy fill closes short position
        assert!(buy_fill.is_closing(PositionDirection::Short));
        // Buy fill does NOT close long position (it would increase it)
        assert!(!buy_fill.is_closing(PositionDirection::Long));
    }

    #[test]
    fn fill_builder_pattern() {
        let fill = Fill::builder(
            OrderId::generate(),
            Symbol::new("MSFT").unwrap(),
            Quantity::new(dec!(200)).unwrap(),
            Price::new(dec!(300.00)).unwrap(),
            Side::Buy,
            Utc::now(),
        )
        .commission(Money::new(dec!(10), Currency::USD).unwrap())
        .build();
        
        assert_eq!(fill.symbol().as_str(), "MSFT");
        assert_eq!(fill.quantity().inner(), dec!(200));
        assert!(fill.commission().is_some());
        assert_eq!(fill.commission().unwrap().amount(), dec!(10));
    }

    #[test]
    fn fill_serde_roundtrip() {
        let fill = create_test_fill_with_commission(Side::Buy, dec!(100), dec!(150.00), dec!(5.00));
        let json = serde_json::to_string(&fill).unwrap();
        let deserialized: Fill = serde_json::from_str(&json).unwrap();
        
        assert_eq!(fill.order_id(), deserialized.order_id());
        assert_eq!(fill.symbol().as_str(), deserialized.symbol().as_str());
        assert_eq!(fill.quantity(), deserialized.quantity());
        assert_eq!(fill.price(), deserialized.price());
        assert_eq!(fill.side(), deserialized.side());
        assert_eq!(fill.commission(), deserialized.commission());
    }

    // =========================================================================
    // FILL AGGREGATION TESTS
    // =========================================================================

    #[test]
    fn fill_aggregation_empty() {
        let agg = FillAggregation::new(Currency::USD);
        
        assert_eq!(agg.total_quantity().inner(), dec!(0));
        assert_eq!(agg.avg_price().inner(), dec!(0));
        assert_eq!(agg.total_notional().amount(), dec!(0));
        assert_eq!(agg.total_commission().amount(), dec!(0));
        assert_eq!(agg.fill_count(), 0);
    }

    #[test]
    fn fill_aggregation_single_fill() {
        let fill = create_test_fill(Side::Buy, dec!(100), dec!(150.00));
        let mut agg = FillAggregation::new(Currency::USD);
        agg.add_fill(&fill);
        
        assert_eq!(agg.total_quantity().inner(), dec!(100));
        assert_eq!(agg.avg_price().inner(), dec!(150.00));
        assert_eq!(agg.total_notional().amount(), dec!(15000));
        assert_eq!(agg.fill_count(), 1);
    }

    #[test]
    fn fill_aggregation_multiple_fills() {
        let fills = vec![
            create_test_fill(Side::Buy, dec!(50), dec!(150.00)),
            create_test_fill(Side::Buy, dec!(50), dec!(152.00)),
        ];
        
        let agg = FillAggregation::from_fills(fills.iter(), Currency::USD);
        
        // Total quantity: 50 + 50 = 100
        assert_eq!(agg.total_quantity().inner(), dec!(100));
        
        // Average price: (50*150 + 50*152) / 100 = 151
        assert_eq!(agg.avg_price().inner(), dec!(151));
        
        // Total notional: $7,500 + $7,600 = $15,100
        assert_eq!(agg.total_notional().amount(), dec!(15100));
        
        assert_eq!(agg.fill_count(), 2);
    }

    #[test]
    fn fill_aggregation_with_commissions() {
        let fills = vec![
            create_test_fill_with_commission(Side::Buy, dec!(50), dec!(150.00), dec!(5.00)),
            create_test_fill_with_commission(Side::Buy, dec!(50), dec!(152.00), dec!(5.00)),
        ];
        
        let agg = FillAggregation::from_fills(fills.iter(), Currency::USD);
        
        // Total commission: $5 + $5 = $10
        assert_eq!(agg.total_commission().amount(), dec!(10));
    }

    #[test]
    fn fill_aggregation_net_value_buy() {
        let fills = vec![
            create_test_fill_with_commission(Side::Buy, dec!(50), dec!(150.00), dec!(5.00)),
            create_test_fill_with_commission(Side::Buy, dec!(50), dec!(152.00), dec!(5.00)),
        ];
        
        let agg = FillAggregation::from_fills(fills.iter(), Currency::USD);
        let net = agg.net_value(Side::Buy);
        
        // Buy: total_notional + commission = $15,100 + $10 = $15,110
        assert_eq!(net.amount(), dec!(15110));
    }

    #[test]
    fn fill_aggregation_net_value_sell() {
        let fills = vec![
            create_test_fill_with_commission(Side::Sell, dec!(50), dec!(160.00), dec!(5.00)),
            create_test_fill_with_commission(Side::Sell, dec!(50), dec!(162.00), dec!(5.00)),
        ];
        
        let agg = FillAggregation::from_fills(fills.iter(), Currency::USD);
        let net = agg.net_value(Side::Sell);
        
        // Sell: total_notional - commission = $16,100 - $10 = $16,090
        assert_eq!(net.amount(), dec!(16090));
    }

    #[test]
    fn fill_aggregation_weighted_average_different_quantities() {
        let fills = vec![
            create_test_fill(Side::Buy, dec!(100), dec!(150.00)),
            create_test_fill(Side::Buy, dec!(50), dec!(180.00)), // Higher price, less quantity
        ];
        
        let agg = FillAggregation::from_fills(fills.iter(), Currency::USD);
        
        // Weighted avg: (100*150 + 50*180) / 150 = (15000 + 9000) / 150 = 160
        assert_eq!(agg.avg_price().inner(), dec!(160));
    }

    #[test]
    fn fill_aggregation_mixed_commission_and_no_commission() {
        let fills = vec![
            create_test_fill_with_commission(Side::Buy, dec!(50), dec!(150.00), dec!(5.00)),
            create_test_fill(Side::Buy, dec!(50), dec!(152.00)), // No commission
        ];
        
        let agg = FillAggregation::from_fills(fills.iter(), Currency::USD);
        
        // Total commission: $5 + $0 = $5
        assert_eq!(agg.total_commission().amount(), dec!(5));
    }

    #[test]
    fn fill_aggregation_serde_roundtrip() {
        let fills = vec![
            create_test_fill(Side::Buy, dec!(50), dec!(150.00)),
            create_test_fill(Side::Buy, dec!(50), dec!(152.00)),
        ];
        
        let agg = FillAggregation::from_fills(fills.iter(), Currency::USD);
        let json = serde_json::to_string(&agg).unwrap();
        let deserialized: FillAggregation = serde_json::from_str(&json).unwrap();
        
        assert_eq!(agg.total_quantity(), deserialized.total_quantity());
        assert_eq!(agg.avg_price(), deserialized.avg_price());
        assert_eq!(agg.total_notional(), deserialized.total_notional());
        assert_eq!(agg.total_commission(), deserialized.total_commission());
        assert_eq!(agg.fill_count(), deserialized.fill_count());
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
        assert!(matches!(order.status(), OrderStatus::Pending { .. }));
        
        let fill_qty = Quantity::new(dec!(5)).unwrap();
        let fill_price = Price::new(dec!(150.0)).unwrap();
        order.fill(fill_qty, fill_price).unwrap();
        assert!(matches!(order.status(), OrderStatus::PartiallyFilled { .. }));
        
        let fill_qty = Quantity::new(dec!(5)).unwrap();
        let fill_price = Price::new(dec!(151.0)).unwrap();
        order.fill(fill_qty, fill_price).unwrap();
        assert!(matches!(order.status(), OrderStatus::Filled { .. }));
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
