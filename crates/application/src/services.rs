//! # Application Services
//!
//! High-level orchestration services that coordinate multiple use cases.

use std::sync::Arc;

use crate::dto::OrderFillDto;
use crate::errors::{ApplicationError, ApplicationResult};
use crate::order_use_cases::FillOrderUseCase;
use crate::ports::{OrderExecutionPort, OrderFillHandler};
use crate::position_use_cases::GetPositionsUseCase;
use domain::{OrderRepository, PositionRepository};

/// Service that orchestrates trading workflows
pub struct TradingService {
    fill_order_use_case: FillOrderUseCase,
    get_positions_use_case: GetPositionsUseCase,
}

impl TradingService {
    /// Creates a new TradingService
    #[must_use]
    pub fn new(
        order_repo: Arc<dyn OrderRepository>,
        position_repo: Arc<dyn PositionRepository>,
        _execution_port: Arc<dyn OrderExecutionPort>,
    ) -> Self {
        // Simplified implementation - in production would wire all dependencies
        use crate::ports::{MarketDataPort, NotificationLevel, NotificationPort};
        use async_trait::async_trait;

        // Mock implementations for compilation
        struct MockNotificationPort;
        #[async_trait]
        impl NotificationPort for MockNotificationPort {
            async fn notify(
                &self,
                _message: &str,
                _level: NotificationLevel,
            ) -> crate::errors::ApplicationResult<()> {
                Ok(())
            }
        }

        let notification_port: Arc<dyn NotificationPort> = Arc::new(MockNotificationPort);

        Self {
            fill_order_use_case: FillOrderUseCase::new(
                order_repo.clone(),
                notification_port.clone(),
            ),
            get_positions_use_case: GetPositionsUseCase::new(
                position_repo,
                Arc::new(MockMarketDataPort),
            ),
        }
    }

    /// Handles an order fill event
    ///
    /// # Errors
    ///
    /// Returns error if handling fails
    pub async fn handle_order_fill(&self, fill: OrderFillDto) -> ApplicationResult<()> {
        self.fill_order_use_case.execute(fill).await?;
        Ok(())
    }
}

struct MockMarketDataPort;

#[async_trait::async_trait]
impl crate::ports::MarketDataPort for MockMarketDataPort {
    async fn get_current_price(&self, _symbol: &str) -> ApplicationResult<rust_decimal::Decimal> {
        Err(ApplicationError::not_implemented("Mock"))
    }

    async fn is_market_open(&self, _symbol: &str) -> ApplicationResult<bool> {
        Ok(true) // Always open for mock
    }

    async fn get_historical_prices(
        &self,
        _symbol: &str,
        _start: chrono::DateTime<chrono::Utc>,
        _end: chrono::DateTime<chrono::Utc>,
    ) -> ApplicationResult<Vec<(chrono::DateTime<chrono::Utc>, rust_decimal::Decimal)>> {
        Err(ApplicationError::not_implemented("Mock"))
    }
}

/// Handler for order fill events from the market
pub struct OrderFillEventHandler {
    fill_order_use_case: FillOrderUseCase,
}

impl OrderFillEventHandler {
    /// Creates a new OrderFillEventHandler
    #[must_use]
    pub fn new(
        order_repo: Arc<dyn OrderRepository>,
        notification_port: Arc<dyn NotificationPort>,
    ) -> Self
    where
        dyn NotificationPort: Send + Sync + 'static,
    {
        Self {
            fill_order_use_case: FillOrderUseCase::new(order_repo, notification_port),
        }
    }
}

use crate::ports::NotificationPort;

#[async_trait::async_trait]
impl OrderFillHandler for OrderFillEventHandler {
    async fn handle_fill(&self, fill: OrderFillDto) -> ApplicationResult<()> {
        self.fill_order_use_case.execute(fill).await?;
        Ok(())
    }
}
