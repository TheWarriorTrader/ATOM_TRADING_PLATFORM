//! # Data Transfer Objects
//!
//! DTOs for transferring data between layers.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Order side (Buy/Sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderSideDto {
    /// Buy order
    Buy,
    /// Sell order
    Sell,
}

/// Order type (Market, Limit, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderTypeDto {
    /// Market order
    Market,
    /// Limit order
    Limit,
    /// Stop order
    Stop,
    /// Stop-limit order
    StopLimit,
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderStatusDto {
    /// Order created locally but not yet submitted
    Created,
    /// Order submitted to broker/exchange
    Submitted,
    /// Order is pending
    Pending,
    /// Order is partially filled
    PartiallyFilled,
    /// Order is completely filled
    Filled,
    /// Order was cancelled
    Cancelled,
    /// Order was rejected
    Rejected,
}

/// DTO for creating an order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderDto {
    /// Account ID
    pub account_id: Uuid,
    /// Symbol
    pub symbol: String,
    /// Order side
    pub side: OrderSideDto,
    /// Order type
    pub order_type: OrderTypeDto,
    /// Order quantity
    pub quantity: Decimal,
    /// Limit price (for limit orders)
    pub limit_price: Option<Decimal>,
    /// Stop price (for stop orders)
    pub stop_price: Option<Decimal>,
}

/// DTO for order response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDto {
    /// Order ID
    pub id: Uuid,
    /// Account ID
    pub account_id: Uuid,
    /// Symbol
    pub symbol: String,
    /// Order side
    pub side: OrderSideDto,
    /// Order type
    pub order_type: OrderTypeDto,
    /// Order quantity
    pub quantity: Decimal,
    /// Filled quantity
    pub filled_quantity: Decimal,
    /// Limit price
    pub limit_price: Option<Decimal>,
    /// Stop price
    pub stop_price: Option<Decimal>,
    /// Current status
    pub status: OrderStatusDto,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Updated timestamp
    pub updated_at: DateTime<Utc>,
}

/// DTO for cancelling an order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelOrderDto {
    /// Order ID
    pub order_id: Uuid,
    /// Optional reason
    pub reason: Option<String>,
}

/// DTO for account response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountDto {
    /// Account ID
    pub id: Uuid,
    /// Account name
    pub name: String,
    /// Account currency
    pub currency: String,
    /// Current balance
    pub balance: Decimal,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

/// DTO for creating an account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAccountDto {
    /// Account name
    pub name: String,
    /// Account currency
    pub currency: String,
    /// Initial balance
    pub initial_balance: Decimal,
}

/// Position direction (Long/Short)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PositionDirectionDto {
    /// Long position
    Long,
    /// Short position
    Short,
}

/// DTO for position response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionDto {
    /// Position ID
    pub id: Uuid,
    /// Account ID
    pub account_id: Uuid,
    /// Symbol
    pub symbol: String,
    /// Position direction
    pub direction: PositionDirectionDto,
    /// Position quantity
    pub quantity: Decimal,
    /// Average entry price
    pub avg_entry_price: Decimal,
    /// Current market price
    pub current_price: Option<Decimal>,
    /// Unrealized P&L
    pub unrealized_pnl: Option<Decimal>,
    /// Realized P&L
    pub realized_pnl: Decimal,
    /// Opened timestamp
    pub opened_at: DateTime<Utc>,
}

/// DTO for order fill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderFillDto {
    /// Order ID
    pub order_id: Uuid,
    /// Fill quantity
    pub fill_quantity: Decimal,
    /// Fill price
    pub fill_price: Decimal,
}

// Conversions from domain types

impl From<domain::entities::OrderSide> for OrderSideDto {
    fn from(side: domain::entities::OrderSide) -> Self {
        match side {
            domain::entities::OrderSide::Buy => Self::Buy,
            domain::entities::OrderSide::Sell => Self::Sell,
        }
    }
}

impl From<OrderSideDto> for domain::entities::OrderSide {
    fn from(side: OrderSideDto) -> Self {
        match side {
            OrderSideDto::Buy => Self::Buy,
            OrderSideDto::Sell => Self::Sell,
        }
    }
}

impl From<domain::entities::OrderType> for OrderTypeDto {
    fn from(order_type: domain::entities::OrderType) -> Self {
        match order_type {
            domain::entities::OrderType::Market => Self::Market,
            domain::entities::OrderType::Limit => Self::Limit,
            domain::entities::OrderType::Stop => Self::Stop,
            domain::entities::OrderType::StopLimit => Self::StopLimit,
            _ => Self::Market, // Fallback for non-exhaustive
        }
    }
}

impl From<domain::entities::OrderStatus> for OrderStatusDto {
    fn from(status: domain::entities::OrderStatus) -> Self {
        match status {
            domain::entities::OrderStatus::Created => Self::Created,
            domain::entities::OrderStatus::Submitted { .. } => Self::Submitted,
            domain::entities::OrderStatus::Pending { .. } => Self::Pending,
            domain::entities::OrderStatus::PartiallyFilled { .. } => Self::PartiallyFilled,
            domain::entities::OrderStatus::Filled { .. } => Self::Filled,
            domain::entities::OrderStatus::Cancelled { .. } => Self::Cancelled,
            domain::entities::OrderStatus::Rejected { .. } => Self::Rejected,
            _ => Self::Pending, // Fallback for future variants
        }
    }
}

impl From<domain::entities::PositionDirection> for PositionDirectionDto {
    fn from(direction: domain::entities::PositionDirection) -> Self {
        match direction {
            domain::entities::PositionDirection::Long => Self::Long,
            domain::entities::PositionDirection::Short => Self::Short,
            _ => Self::Long, // Fallback for non-exhaustive
        }
    }
}
