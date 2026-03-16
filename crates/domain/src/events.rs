//! # Domain Events
//!
//! Events representing significant occurrences in the domain.

use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entities::{EntityId, OrderSide, PositionDirection};
use crate::values::{Currency, OrderId, Price, Quantity, Side, Symbol};

/// Base trait for all domain events
pub trait DomainEvent: Send + Sync {
    /// Returns the event ID
    fn event_id(&self) -> EntityId;
    /// Returns the timestamp when the event occurred
    fn occurred_at(&self) -> DateTime<Utc>;
}

/// Event emitted when an order is created
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderCreated {
    /// Event ID
    pub event_id: EntityId,
    /// Order ID
    pub order_id: EntityId,
    /// Account ID
    pub account_id: EntityId,
    /// Symbol
    pub symbol: Symbol,
    /// Order side
    pub side: OrderSide,
    /// Order quantity
    pub quantity: Quantity,
    /// Timestamp
    pub occurred_at: DateTime<Utc>,
}

impl OrderCreated {
    /// Creates a new OrderCreated event
    #[must_use]
    pub fn new(
        order_id: EntityId,
        account_id: EntityId,
        symbol: Symbol,
        side: OrderSide,
        quantity: Quantity,
    ) -> Self {
        Self {
            event_id: EntityId::new_v4(),
            order_id,
            account_id,
            symbol,
            side,
            quantity,
            occurred_at: Utc::now(),
        }
    }
}

impl DomainEvent for OrderCreated {
    fn event_id(&self) -> EntityId {
        self.event_id
    }

    fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }
}

/// Event emitted when an order is filled
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderFilled {
    /// Event ID
    pub event_id: EntityId,
    /// Order ID
    pub order_id: EntityId,
    /// Account ID
    pub account_id: EntityId,
    /// Symbol
    pub symbol: Symbol,
    /// Fill quantity
    pub fill_quantity: Quantity,
    /// Fill price
    pub fill_price: Price,
    /// Total filled quantity
    pub total_filled: Quantity,
    /// Is fully filled
    pub is_fully_filled: bool,
    /// Timestamp
    pub occurred_at: DateTime<Utc>,
}

impl OrderFilled {
    /// Creates a new OrderFilled event
    #[must_use]
    pub fn new(
        order_id: EntityId,
        account_id: EntityId,
        symbol: Symbol,
        fill_quantity: Quantity,
        fill_price: Price,
        total_filled: Quantity,
        is_fully_filled: bool,
    ) -> Self {
        Self {
            event_id: EntityId::new_v4(),
            order_id,
            account_id,
            symbol,
            fill_quantity,
            fill_price,
            total_filled,
            is_fully_filled,
            occurred_at: Utc::now(),
        }
    }
}

impl DomainEvent for OrderFilled {
    fn event_id(&self) -> EntityId {
        self.event_id
    }

    fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }
}

/// Event emitted when an order is cancelled
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderCancelled {
    /// Event ID
    pub event_id: EntityId,
    /// Order ID
    pub order_id: EntityId,
    /// Account ID
    pub account_id: EntityId,
    /// Reason for cancellation
    pub reason: Option<String>,
    /// Timestamp
    pub occurred_at: DateTime<Utc>,
}

impl OrderCancelled {
    /// Creates a new OrderCancelled event
    #[must_use]
    pub fn new(
        order_id: EntityId,
        account_id: EntityId,
        reason: impl Into<Option<String>>,
    ) -> Self {
        Self {
            event_id: EntityId::new_v4(),
            order_id,
            account_id,
            reason: reason.into(),
            occurred_at: Utc::now(),
        }
    }
}

impl DomainEvent for OrderCancelled {
    fn event_id(&self) -> EntityId {
        self.event_id
    }

    fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }
}

/// Event emitted when a position is opened
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionOpened {
    /// Event ID
    pub event_id: EntityId,
    /// Position ID
    pub position_id: EntityId,
    /// Account ID
    pub account_id: EntityId,
    /// Symbol
    pub symbol: Symbol,
    /// Position direction
    pub direction: PositionDirection,
    /// Entry quantity
    pub quantity: Quantity,
    /// Entry price
    pub entry_price: Price,
    /// Timestamp
    pub occurred_at: DateTime<Utc>,
}

impl PositionOpened {
    /// Creates a new PositionOpened event
    #[must_use]
    pub fn new(
        position_id: EntityId,
        account_id: EntityId,
        symbol: Symbol,
        direction: PositionDirection,
        quantity: Quantity,
        entry_price: Price,
    ) -> Self {
        Self {
            event_id: EntityId::new_v4(),
            position_id,
            account_id,
            symbol,
            direction,
            quantity,
            entry_price,
            occurred_at: Utc::now(),
        }
    }
}

impl DomainEvent for PositionOpened {
    fn event_id(&self) -> EntityId {
        self.event_id
    }

    fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }
}

/// Event emitted when a position is closed
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionClosed {
    /// Event ID
    pub event_id: EntityId,
    /// Position ID
    pub position_id: EntityId,
    /// Account ID
    pub account_id: EntityId,
    /// Symbol
    pub symbol: Symbol,
    /// Closed quantity
    pub quantity: Quantity,
    /// Exit price
    pub exit_price: Price,
    /// Realized P&L
    pub realized_pnl: Decimal,
    /// Timestamp
    pub occurred_at: DateTime<Utc>,
}

impl PositionClosed {
    /// Creates a new PositionClosed event
    #[must_use]
    pub fn new(
        position_id: EntityId,
        account_id: EntityId,
        symbol: Symbol,
        quantity: Quantity,
        exit_price: Price,
        realized_pnl: Decimal,
    ) -> Self {
        Self {
            event_id: EntityId::new_v4(),
            position_id,
            account_id,
            symbol,
            quantity,
            exit_price,
            realized_pnl,
            occurred_at: Utc::now(),
        }
    }
}

impl DomainEvent for PositionClosed {
    fn event_id(&self) -> EntityId {
        self.event_id
    }

    fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }
}

/// Event emitted when a position is updated (size changed)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionUpdated {
    /// Event ID
    pub event_id: EntityId,
    /// Position ID
    pub position_id: EntityId,
    /// Account ID
    pub account_id: EntityId,
    /// New quantity
    pub new_quantity: Quantity,
    /// New average entry price
    pub new_avg_price: Price,
    /// Timestamp
    pub occurred_at: DateTime<Utc>,
}

impl PositionUpdated {
    /// Creates a new PositionUpdated event
    #[must_use]
    pub fn new(
        position_id: EntityId,
        account_id: EntityId,
        new_quantity: Quantity,
        new_avg_price: Price,
    ) -> Self {
        Self {
            event_id: EntityId::new_v4(),
            position_id,
            account_id,
            new_quantity,
            new_avg_price,
            occurred_at: Utc::now(),
        }
    }
}

impl DomainEvent for PositionUpdated {
    fn event_id(&self) -> EntityId {
        self.event_id
    }

    fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }
}

/// Event emitted when account balance changes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountBalanceUpdated {
    /// Event ID
    pub event_id: EntityId,
    /// Account ID
    pub account_id: EntityId,
    /// Old balance
    pub old_balance: Decimal,
    /// New balance
    pub new_balance: Decimal,
    /// Change reason
    pub reason: String,
    /// Timestamp
    pub occurred_at: DateTime<Utc>,
}

impl AccountBalanceUpdated {
    /// Creates a new AccountBalanceUpdated event
    #[must_use]
    pub fn new(
        account_id: EntityId,
        old_balance: Decimal,
        new_balance: Decimal,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            event_id: EntityId::new_v4(),
            account_id,
            old_balance,
            new_balance,
            reason: reason.into(),
            occurred_at: Utc::now(),
        }
    }
}

impl DomainEvent for AccountBalanceUpdated {
    fn event_id(&self) -> EntityId {
        self.event_id
    }

    fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }
}

// =============================================================================
// EVENT METADATA
// =============================================================================

/// Metadata attached to domain events for tracking and auditing.
///
/// Provides correlation and causation IDs for event tracing, enabling
/// end-to-end tracking of event flows across the system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Unique event identifier
    pub event_id: Uuid,
    /// Correlation ID for tracking related events
    pub correlation_id: Uuid,
    /// ID of the event that caused this event (causation)
    pub causation_id: Option<Uuid>,
    /// Timestamp when the event was created
    pub timestamp: DateTime<Utc>,
    /// Source of the event (component/service name)
    pub source: String,
    /// Optional user ID associated with the event
    pub user_id: Option<String>,
}

impl EventMetadata {
    /// Creates new metadata with a generated correlation ID.
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        let event_id = Uuid::new_v4();
        Self {
            correlation_id: event_id,
            event_id,
            causation_id: None,
            timestamp: Utc::now(),
            source: source.into(),
            user_id: None,
        }
    }

    /// Creates new metadata with a specific correlation ID.
    #[must_use]
    pub fn with_correlation(correlation_id: Uuid, source: impl Into<String>) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            correlation_id,
            causation_id: None,
            timestamp: Utc::now(),
            source: source.into(),
            user_id: None,
        }
    }

    /// Sets the causation ID (the event that caused this event).
    #[must_use]
    pub fn caused_by(mut self, causation_id: Uuid) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    /// Sets the user ID.
    #[must_use]
    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self::new("unknown")
    }
}

// =============================================================================
// HELPER STRUCTS
// =============================================================================

/// Trading signal generated by a strategy.
///
/// Represents a buy/sell recommendation with optional metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    /// Signal side (buy/sell)
    pub side: Side,
    /// Signal strength (0.0 to 1.0)
    pub strength: f64,
    /// Recommended quantity
    pub quantity: Option<Quantity>,
    /// Signal price (if limit)
    pub price: Option<Price>,
    /// Signal type
    pub signal_type: SignalType,
    /// Strategy-specific metadata
    pub metadata: Option<HashMap<String, String>>,
}

impl Signal {
    /// Creates a new signal.
    ///
    /// # Panics
    /// Panics if strength is not in range [0.0, 1.0].
    #[must_use]
    pub fn new(side: Side, strength: f64, signal_type: SignalType) -> Self {
        assert!(
            (0.0..=1.0).contains(&strength),
            "Signal strength must be in range [0.0, 1.0]"
        );
        Self {
            side,
            strength,
            quantity: None,
            price: None,
            signal_type,
            metadata: None,
        }
    }

    /// Sets the quantity for the signal.
    #[must_use]
    pub fn with_quantity(mut self, quantity: Quantity) -> Self {
        self.quantity = Some(quantity);
        self
    }

    /// Sets the price for the signal.
    #[must_use]
    pub fn with_price(mut self, price: Price) -> Self {
        self.price = Some(price);
        self
    }

    /// Sets metadata for the signal.
    #[must_use]
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Type of trading signal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SignalType {
    /// Entry signal (open new position)
    Entry,
    /// Exit signal (close existing position)
    Exit,
    /// Adjust position size
    Adjust,
    /// Stop loss triggered
    StopLoss,
    /// Take profit triggered
    TakeProfit,
    /// Trailing stop triggered
    TrailingStop,
    /// Custom signal type
    Custom(String),
}

impl fmt::Display for SignalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entry => write!(f, "Entry"),
            Self::Exit => write!(f, "Exit"),
            Self::Adjust => write!(f, "Adjust"),
            Self::StopLoss => write!(f, "StopLoss"),
            Self::TakeProfit => write!(f, "TakeProfit"),
            Self::TrailingStop => write!(f, "TrailingStop"),
            Self::Custom(s) => write!(f, "Custom({s})"),
        }
    }
}

/// Represents a change in position state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PositionChange {
    /// New position opened
    Opened,
    /// Position size increased
    Increased,
    /// Position size decreased
    Decreased,
    /// Position closed
    Closed,
    /// Position flipped (long to short or vice versa)
    Flipped,
}

impl fmt::Display for PositionChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Opened => write!(f, "Opened"),
            Self::Increased => write!(f, "Increased"),
            Self::Decreased => write!(f, "Decreased"),
            Self::Closed => write!(f, "Closed"),
            Self::Flipped => write!(f, "Flipped"),
        }
    }
}

/// Represents a change in account state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AccountChange {
    /// Balance changed
    Balance { old: Money, new: Money },
    /// Buying power changed
    BuyingPower { old: Money, new: Money },
    /// Margin used changed
    MarginUsed { old: Money, new: Money },
    /// Currency changed
    Currency(Currency),
    /// Status changed
    Status { from: String, to: String },
}

impl fmt::Display for AccountChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Balance { old, new } => {
                write!(f, "Balance changed from {old} to {new}")
            }
            Self::BuyingPower { old, new } => {
                write!(f, "Buying power changed from {old} to {new}")
            }
            Self::MarginUsed { old, new } => {
                write!(f, "Margin used changed from {old} to {new}")
            }
            Self::Currency(currency) => write!(f, "Currency set to {currency}"),
            Self::Status { from, to } => write!(f, "Status changed from {from} to {to}"),
        }
    }
}

// =============================================================================
// TRADING EVENT ENUM
// =============================================================================

use crate::entities::{Bar, ClosedPosition, Fill, Order, Position, Tick};
use crate::values::Money;

/// Main enum representing all trading domain events.
///
/// This is the primary event type used throughout the system for
/// event-driven architecture. All events are immutable, serializable,
/// and timestamped for audit trails.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TradingEvent {
    /// New OHLCV bar received from data provider
    BarReceived {
        /// The bar data
        bar: Bar,
        /// Data source/provider
        source: String,
        /// Event timestamp
        timestamp: DateTime<Utc>,
    },
    /// New tick received from data provider
    TickReceived {
        /// The tick data
        tick: Tick,
        /// Data source/provider
        source: String,
        /// Event timestamp
        timestamp: DateTime<Utc>,
    },
    /// Order submitted to broker
    OrderSubmitted {
        /// Order ID
        order_id: OrderId,
        /// Order details
        order: Order,
        /// Event timestamp
        timestamp: DateTime<Utc>,
    },
    /// Order filled (fully or partially)
    OrderFilled {
        /// Order ID
        order_id: OrderId,
        /// Fill details
        fill: Fill,
        /// Event timestamp
        timestamp: DateTime<Utc>,
    },
    /// Order rejected by broker
    OrderRejected {
        /// Order ID
        order_id: OrderId,
        /// Rejection reason
        reason: String,
        /// Event timestamp
        timestamp: DateTime<Utc>,
    },
    /// Order cancelled
    OrderCancelled {
        /// Order ID
        order_id: OrderId,
        /// Cancellation reason
        reason: String,
        /// Event timestamp
        timestamp: DateTime<Utc>,
    },
    /// Position updated (size changed)
    PositionUpdated {
        /// Trading symbol
        symbol: Symbol,
        /// Current position state
        position: Position,
        /// Type of change
        change: PositionChange,
        /// Event timestamp
        timestamp: DateTime<Utc>,
    },
    /// Position closed
    PositionClosed {
        /// Trading symbol
        symbol: Symbol,
        /// Closed position details
        closed_position: ClosedPosition,
        /// Realized P&L
        realized_pnl: Money,
        /// Event timestamp
        timestamp: DateTime<Utc>,
    },
    /// Trading signal generated by strategy
    SignalGenerated {
        /// Strategy name/identifier
        strategy: String,
        /// Signal details
        signal: Signal,
        /// Event timestamp
        timestamp: DateTime<Utc>,
    },
    /// Account state updated
    AccountUpdated {
        /// Account ID
        account_id: EntityId,
        /// List of changes
        changes: Vec<AccountChange>,
        /// Event timestamp
        timestamp: DateTime<Utc>,
    },
}

impl TradingEvent {
    /// Returns the event timestamp.
    #[must_use]
    pub const fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::BarReceived { timestamp, .. } => *timestamp,
            Self::TickReceived { timestamp, .. } => *timestamp,
            Self::OrderSubmitted { timestamp, .. } => *timestamp,
            Self::OrderFilled { timestamp, .. } => *timestamp,
            Self::OrderRejected { timestamp, .. } => *timestamp,
            Self::OrderCancelled { timestamp, .. } => *timestamp,
            Self::PositionUpdated { timestamp, .. } => *timestamp,
            Self::PositionClosed { timestamp, .. } => *timestamp,
            Self::SignalGenerated { timestamp, .. } => *timestamp,
            Self::AccountUpdated { timestamp, .. } => *timestamp,
        }
    }

    /// Returns the event type as a static string.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::BarReceived { .. } => "bar_received",
            Self::TickReceived { .. } => "tick_received",
            Self::OrderSubmitted { .. } => "order_submitted",
            Self::OrderFilled { .. } => "order_filled",
            Self::OrderRejected { .. } => "order_rejected",
            Self::OrderCancelled { .. } => "order_cancelled",
            Self::PositionUpdated { .. } => "position_updated",
            Self::PositionClosed { .. } => "position_closed",
            Self::SignalGenerated { .. } => "signal_generated",
            Self::AccountUpdated { .. } => "account_updated",
        }
    }

    /// Returns true if this is a market data event.
    #[must_use]
    pub const fn is_market_data(&self) -> bool {
        matches!(self, Self::BarReceived { .. } | Self::TickReceived { .. })
    }

    /// Returns true if this is an order event.
    #[must_use]
    pub const fn is_order_event(&self) -> bool {
        matches!(
            self,
            Self::OrderSubmitted { .. }
                | Self::OrderFilled { .. }
                | Self::OrderRejected { .. }
                | Self::OrderCancelled { .. }
        )
    }

    /// Returns true if this is a position event.
    #[must_use]
    pub const fn is_position_event(&self) -> bool {
        matches!(self, Self::PositionUpdated { .. } | Self::PositionClosed { .. })
    }

    /// Returns true if this is a signal event.
    #[must_use]
    pub const fn is_signal(&self) -> bool {
        matches!(self, Self::SignalGenerated { .. })
    }

    /// Creates a BarReceived event.
    #[must_use]
    pub fn bar_received(bar: Bar, source: impl Into<String>) -> Self {
        Self::BarReceived {
            bar,
            source: source.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a TickReceived event.
    #[must_use]
    pub fn tick_received(tick: Tick, source: impl Into<String>) -> Self {
        Self::TickReceived {
            tick,
            source: source.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates an OrderSubmitted event.
    #[must_use]
    pub fn order_submitted(order_id: OrderId, order: Order) -> Self {
        Self::OrderSubmitted {
            order_id,
            order,
            timestamp: Utc::now(),
        }
    }

    /// Creates an OrderFilled event.
    #[must_use]
    pub fn order_filled(order_id: OrderId, fill: Fill) -> Self {
        Self::OrderFilled {
            order_id,
            fill,
            timestamp: Utc::now(),
        }
    }

    /// Creates an OrderRejected event.
    #[must_use]
    pub fn order_rejected(order_id: OrderId, reason: impl Into<String>) -> Self {
        Self::OrderRejected {
            order_id,
            reason: reason.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates an OrderCancelled event.
    #[must_use]
    pub fn order_cancelled(order_id: OrderId, reason: impl Into<String>) -> Self {
        Self::OrderCancelled {
            order_id,
            reason: reason.into(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a PositionUpdated event.
    #[must_use]
    pub fn position_updated(
        symbol: Symbol,
        position: Position,
        change: PositionChange,
    ) -> Self {
        Self::PositionUpdated {
            symbol,
            position,
            change,
            timestamp: Utc::now(),
        }
    }

    /// Creates a PositionClosed event.
    #[must_use]
    pub fn position_closed(
        symbol: Symbol,
        closed_position: ClosedPosition,
        realized_pnl: Money,
    ) -> Self {
        Self::PositionClosed {
            symbol,
            closed_position,
            realized_pnl,
            timestamp: Utc::now(),
        }
    }

    /// Creates a SignalGenerated event.
    #[must_use]
    pub fn signal_generated(strategy: impl Into<String>, signal: Signal) -> Self {
        Self::SignalGenerated {
            strategy: strategy.into(),
            signal,
            timestamp: Utc::now(),
        }
    }

    /// Creates an AccountUpdated event.
    #[must_use]
    pub fn account_updated(account_id: EntityId, changes: Vec<AccountChange>) -> Self {
        Self::AccountUpdated {
            account_id,
            changes,
            timestamp: Utc::now(),
        }
    }
}

// =============================================================================
// FROM TRAITS
// =============================================================================

impl From<Bar> for TradingEvent {
    fn from(bar: Bar) -> Self {
        Self::bar_received(bar, "unknown")
    }
}

impl From<Tick> for TradingEvent {
    fn from(tick: Tick) -> Self {
        Self::tick_received(tick, "unknown")
    }
}

impl From<(OrderId, Order)> for TradingEvent {
    fn from((order_id, order): (OrderId, Order)) -> Self {
        Self::order_submitted(order_id, order)
    }
}

impl From<(OrderId, Fill)> for TradingEvent {
    fn from((order_id, fill): (OrderId, Fill)) -> Self {
        Self::order_filled(order_id, fill)
    }
}

impl From<Signal> for TradingEvent {
    fn from(signal: Signal) -> Self {
        Self::signal_generated("unknown", signal)
    }
}

// =============================================================================
// UNIT TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{Bar, Fill, Order, OrderType, PositionDirection, Position, Tick};
    use crate::values::{Currency, OrderId, Price, Quantity, Side, Symbol, TimeFrame, Volume};
    use rust_decimal::Decimal;

    fn create_test_bar() -> Bar {
        Bar::new(
            Utc::now(),
            Symbol::new("AAPL").unwrap(),
            Price::new(Decimal::new(15000, 2)).unwrap(),
            Price::new(Decimal::new(15100, 2)).unwrap(),
            Price::new(Decimal::new(14900, 2)).unwrap(),
            Price::new(Decimal::new(15050, 2)).unwrap(),
            Volume::new(1000).unwrap(),
            TimeFrame::M1,
            "TestProvider",
        )
        .unwrap()
    }

    fn create_test_tick() -> Tick {
        Tick::new(
            Utc::now(),
            Symbol::new("AAPL").unwrap(),
            Price::new(Decimal::new(15050, 2)).unwrap(),
            Volume::new(100).unwrap(),
            Some(Side::Buy),
            "TestProvider",
        )
        .unwrap()
    }

    fn create_test_order() -> Order {
        Order::new_with_id(
            OrderId::generate(),
            Uuid::new_v4(),
            Symbol::new("AAPL").unwrap(),
            Side::Buy,
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            OrderType::Market,
            crate::values::TimeInForce::Day,
            None,
            None,
        )
        .unwrap()
    }

    fn create_test_fill() -> Fill {
        Fill::new(
            OrderId::generate(),
            Symbol::new("AAPL").unwrap(),
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            Price::new(Decimal::new(15050, 2)).unwrap(),
            Side::Buy,
            Utc::now(),
        )
    }

    fn create_test_position() -> Position {
        Position::new(
            Uuid::new_v4(),
            Symbol::new("AAPL").unwrap(),
            PositionDirection::Long,
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            Price::new(Decimal::new(15000, 2)).unwrap(),
            Currency::USD,
        )
    }

    #[test]
    fn bar_received_event() {
        let bar = create_test_bar();
        let event = TradingEvent::from(bar.clone());

        assert_eq!(event.event_type(), "bar_received");
        assert!(matches!(event, TradingEvent::BarReceived { .. }));
    }

    #[test]
    fn tick_received_event() {
        let tick = create_test_tick();
        let event = TradingEvent::from(tick.clone());

        assert_eq!(event.event_type(), "tick_received");
        assert!(matches!(event, TradingEvent::TickReceived { .. }));
    }

    #[test]
    fn order_submitted_event() {
        let order = create_test_order();
        let order_id = OrderId::generate();
        let event = TradingEvent::order_submitted(order_id, order);

        assert_eq!(event.event_type(), "order_submitted");
        assert!(matches!(event, TradingEvent::OrderSubmitted { .. }));
    }

    #[test]
    fn order_filled_event() {
        let order_id = OrderId::generate();
        let fill = create_test_fill();
        let event = TradingEvent::order_filled(order_id, fill);

        assert_eq!(event.event_type(), "order_filled");
        assert!(matches!(event, TradingEvent::OrderFilled { .. }));
    }

    #[test]
    fn order_rejected_event() {
        let order_id = OrderId::generate();
        let event = TradingEvent::order_rejected(order_id, "Insufficient funds");

        assert_eq!(event.event_type(), "order_rejected");
        assert!(matches!(event, TradingEvent::OrderRejected { reason, .. } if reason == "Insufficient funds"));
    }

    #[test]
    fn order_cancelled_event() {
        let order_id = OrderId::generate();
        let event = TradingEvent::order_cancelled(order_id, "User request");

        assert_eq!(event.event_type(), "order_cancelled");
        assert!(matches!(event, TradingEvent::OrderCancelled { reason, .. } if reason == "User request"));
    }

    #[test]
    fn position_updated_event() {
        let position = create_test_position();
        let event = TradingEvent::position_updated(
            Symbol::new("AAPL").unwrap(),
            position,
            PositionChange::Increased,
        );

        assert_eq!(event.event_type(), "position_updated");
        assert!(matches!(event, TradingEvent::PositionUpdated { change, .. } if matches!(change, PositionChange::Increased)));
    }

    #[test]
    fn signal_generated_event() {
        let signal = Signal::new(Side::Buy, 0.85, SignalType::Entry);
        let event = TradingEvent::signal_generated("TestStrategy", signal);

        assert_eq!(event.event_type(), "signal_generated");
        assert!(matches!(event, TradingEvent::SignalGenerated { strategy, .. } if strategy == "TestStrategy"));
    }

    #[test]
    fn account_updated_event() {
        let account_id = Uuid::new_v4();
        let changes = vec![AccountChange::Currency(Currency::USD)];
        let event = TradingEvent::account_updated(account_id, changes);

        assert_eq!(event.event_type(), "account_updated");
        assert!(matches!(event, TradingEvent::AccountUpdated { .. }));
    }

    #[test]
    fn event_timestamp_is_set() {
        let before = Utc::now();
        let bar = create_test_bar();
        let event = TradingEvent::from(bar);
        let after = Utc::now();

        let ts = event.timestamp();
        assert!(ts >= before);
        assert!(ts <= after);
    }

    #[test]
    fn event_is_market_data() {
        let bar = create_test_bar();
        let tick = create_test_tick();

        let bar_event = TradingEvent::from(bar);
        let tick_event = TradingEvent::from(tick);

        assert!(bar_event.is_market_data());
        assert!(tick_event.is_market_data());

        let order = create_test_order();
        let order_event = TradingEvent::order_submitted(OrderId::generate(), order);
        assert!(!order_event.is_market_data());
    }

    #[test]
    fn event_is_order_event() {
        let order = create_test_order();
        let fill = create_test_fill();
        let order_id = OrderId::generate();

        let submitted = TradingEvent::order_submitted(order_id, order);
        let filled = TradingEvent::order_filled(order_id, fill);
        let rejected = TradingEvent::order_rejected(order_id, "test");
        let cancelled = TradingEvent::order_cancelled(order_id, "test");

        assert!(submitted.is_order_event());
        assert!(filled.is_order_event());
        assert!(rejected.is_order_event());
        assert!(cancelled.is_order_event());

        let bar = create_test_bar();
        let bar_event = TradingEvent::from(bar);
        assert!(!bar_event.is_order_event());
    }

    #[test]
    fn event_is_position_event() {
        let position = create_test_position();
        let symbol = Symbol::new("AAPL").unwrap();

        let updated = TradingEvent::position_updated(symbol.clone(), position.clone(), PositionChange::Increased);
        assert!(updated.is_position_event());

        let bar = create_test_bar();
        let bar_event = TradingEvent::from(bar);
        assert!(!bar_event.is_position_event());
    }

    #[test]
    fn event_is_signal() {
        let signal = Signal::new(Side::Buy, 0.85, SignalType::Entry);
        let signal_event = TradingEvent::signal_generated("Test", signal);
        assert!(signal_event.is_signal());

        let bar = create_test_bar();
        let bar_event = TradingEvent::from(bar);
        assert!(!bar_event.is_signal());
    }

    #[test]
    fn event_serde_roundtrip_bar_received() {
        let bar = create_test_bar();
        let event = TradingEvent::bar_received(bar, "TestProvider");

        let json = serde_json::to_string(&event).unwrap();
        let decoded: TradingEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event.event_type(), decoded.event_type());
        assert_eq!(event.timestamp(), decoded.timestamp());
    }

    #[test]
    fn event_serde_roundtrip_order_filled() {
        let order_id = OrderId::generate();
        let fill = create_test_fill();
        let event = TradingEvent::order_filled(order_id, fill);

        let json = serde_json::to_string(&event).unwrap();
        let decoded: TradingEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event, decoded);
    }

    #[test]
    fn event_serde_roundtrip_signal_generated() {
        let signal = Signal::new(Side::Sell, 0.75, SignalType::Exit)
            .with_quantity(Quantity::new(Decimal::new(50, 0)).unwrap());
        let event = TradingEvent::signal_generated("Strategy1", signal);

        let json = serde_json::to_string(&event).unwrap();
        let decoded: TradingEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event, decoded);
    }

    #[test]
    fn event_clone_works() {
        let bar = create_test_bar();
        let event = TradingEvent::from(bar);
        let cloned = event.clone();

        assert_eq!(event, cloned);
    }

    #[test]
    fn event_metadata_creation() {
        let meta = EventMetadata::new("test_service");

        assert_eq!(meta.source, "test_service");
        assert!(meta.user_id.is_none());
        assert!(meta.causation_id.is_none());
        assert_eq!(meta.event_id, meta.correlation_id); // Same for root events
    }

    #[test]
    fn event_metadata_with_correlation() {
        let correlation_id = Uuid::new_v4();
        let meta = EventMetadata::with_correlation(correlation_id, "test_service");

        assert_eq!(meta.correlation_id, correlation_id);
        assert_ne!(meta.event_id, correlation_id);
    }

    #[test]
    fn event_metadata_with_causation() {
        let causation_id = Uuid::new_v4();
        let meta = EventMetadata::new("test_service").caused_by(causation_id);

        assert_eq!(meta.causation_id, Some(causation_id));
    }

    #[test]
    fn event_metadata_with_user() {
        let meta = EventMetadata::new("test_service").with_user("user123");

        assert_eq!(meta.user_id, Some("user123".to_string()));
    }

    #[test]
    fn signal_creation() {
        let signal = Signal::new(Side::Buy, 0.9, SignalType::Entry)
            .with_quantity(Quantity::new(Decimal::new(100, 0)).unwrap())
            .with_price(Price::new(Decimal::new(15000, 2)).unwrap());

        assert_eq!(signal.side, Side::Buy);
        assert!((signal.strength - 0.9).abs() < f64::EPSILON);
        assert!(matches!(signal.signal_type, SignalType::Entry));
        assert!(signal.quantity.is_some());
        assert!(signal.price.is_some());
    }

    #[test]
    fn position_change_display() {
        assert_eq!(PositionChange::Opened.to_string(), "Opened");
        assert_eq!(PositionChange::Increased.to_string(), "Increased");
        assert_eq!(PositionChange::Decreased.to_string(), "Decreased");
        assert_eq!(PositionChange::Closed.to_string(), "Closed");
        assert_eq!(PositionChange::Flipped.to_string(), "Flipped");
    }

    #[test]
    fn signal_type_display() {
        assert_eq!(SignalType::Entry.to_string(), "Entry");
        assert_eq!(SignalType::Exit.to_string(), "Exit");
        assert_eq!(SignalType::StopLoss.to_string(), "StopLoss");
        assert_eq!(SignalType::Custom("CustomType".to_string()).to_string(), "Custom(CustomType)");
    }

    #[test]
    fn from_traits_bar() {
        let bar = create_test_bar();
        let event: TradingEvent = bar.into();

        assert!(matches!(event, TradingEvent::BarReceived { source, .. } if source == "unknown"));
    }

    #[test]
    fn from_traits_tick() {
        let tick = create_test_tick();
        let event: TradingEvent = tick.into();

        assert!(matches!(event, TradingEvent::TickReceived { source, .. } if source == "unknown"));
    }

    #[test]
    fn from_traits_order_submitted() {
        let order = create_test_order();
        let order_id = OrderId::generate();
        let event: TradingEvent = (order_id, order).into();

        assert!(matches!(event, TradingEvent::OrderSubmitted { .. }));
    }

    #[test]
    fn from_traits_order_filled() {
        let fill = create_test_fill();
        let order_id = OrderId::generate();
        let event: TradingEvent = (order_id, fill).into();

        assert!(matches!(event, TradingEvent::OrderFilled { .. }));
    }

    #[test]
    fn from_traits_signal() {
        let signal = Signal::new(Side::Buy, 0.8, SignalType::Entry);
        let event: TradingEvent = signal.into();

        assert!(matches!(event, TradingEvent::SignalGenerated { strategy, .. } if strategy == "unknown"));
    }
}
