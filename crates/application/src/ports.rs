//! # Application Ports
//!
//! Interfaces that define how the application layer interacts with infrastructure.
//! These are implemented by the infrastructure layer (dependency inversion).

use async_trait::async_trait;

use crate::dto::OrderFillDto;
use crate::errors::ApplicationResult;

/// Port for order execution
#[async_trait]
pub trait OrderExecutionPort: Send + Sync {
    /// Submits an order to the market
    ///
    /// # Errors
    ///
    /// Returns error if submission fails
    async fn submit_order(&self, order_id: uuid::Uuid) -> ApplicationResult<()>;

    /// Cancels an order in the market
    ///
    /// # Errors
    ///
    /// Returns error if cancellation fails
    async fn cancel_order(&self, order_id: uuid::Uuid) -> ApplicationResult<()>;

    /// Gets the current market status
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn is_market_open(&self, symbol: &str) -> ApplicationResult<bool>;
}

/// Port for market data
#[async_trait]
pub trait MarketDataPort: Send + Sync {
    /// Gets the current price for a symbol
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn get_current_price(&self, symbol: &str) -> ApplicationResult<rust_decimal::Decimal>;

    /// Checks if the market is open for a symbol
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn is_market_open(&self, symbol: &str) -> ApplicationResult<bool>;

    /// Gets historical prices for a symbol
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn get_historical_prices(
        &self,
        symbol: &str,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> ApplicationResult<Vec<(chrono::DateTime<chrono::Utc>, rust_decimal::Decimal)>>;
}

/// Port for event publishing
#[async_trait]
pub trait EventPublisherPort: Send + Sync {
    /// Publishes a domain event
    ///
    /// # Errors
    ///
    /// Returns error if publishing fails
    async fn publish_event(&self, event: &str) -> ApplicationResult<()>;
}

/// Port for notification
#[async_trait]
pub trait NotificationPort: Send + Sync {
    /// Sends a notification
    ///
    /// # Errors
    ///
    /// Returns error if sending fails
    async fn notify(&self, message: &str, level: NotificationLevel) -> ApplicationResult<()>;
}

/// Notification level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotificationLevel {
    /// Info level
    Info,
    /// Warning level
    Warning,
    /// Error level
    Error,
    /// Critical level
    Critical,
}

/// Port for order fill callbacks (from market)
#[async_trait]
pub trait OrderFillHandler: Send + Sync {
    /// Handles an order fill from the market
    ///
    /// # Errors
    ///
    /// Returns error if handling fails
    async fn handle_fill(&self, fill: OrderFillDto) -> ApplicationResult<()>;
}
