//! # Order Use Cases
//!
//! Use cases for order operations.

use std::sync::Arc;

use tracing::{info, warn};

use crate::dto::{CreateOrderDto, OrderDto, OrderFillDto, OrderStatusDto};
use crate::errors::{ApplicationError, ApplicationResult};
use crate::ports::{MarketDataPort, NotificationLevel, NotificationPort, OrderExecutionPort};
use domain::{Currency, EntityId, OrderRepository, OrderService, Price, Quantity, Symbol};

/// Use case for creating orders
pub struct CreateOrderUseCase {
    order_service: OrderService,
    order_repo: Arc<dyn OrderRepository>,
    execution_port: Arc<dyn OrderExecutionPort>,
    market_data_port: Arc<dyn MarketDataPort>,
    notification_port: Arc<dyn NotificationPort>,
}

impl CreateOrderUseCase {
    /// Creates a new CreateOrderUseCase
    #[must_use]
    pub fn new(
        order_repo: Arc<dyn OrderRepository>,
        execution_port: Arc<dyn OrderExecutionPort>,
        market_data_port: Arc<dyn MarketDataPort>,
        notification_port: Arc<dyn NotificationPort>,
    ) -> Self {
        Self {
            order_service: OrderService::new(),
            order_repo,
            execution_port,
            market_data_port,
            notification_port,
        }
    }

    /// Executes the use case
    ///
    /// # Errors
    ///
    /// Returns error if order creation fails
    pub async fn execute(&self, dto: CreateOrderDto) -> ApplicationResult<OrderDto> {
        // Validate symbol exists and market is open
        let is_open = self
            .market_data_port
            .is_market_open(&dto.symbol)
            .await
            .map_err(|e| ApplicationError::infrastructure(e.to_string()))?;

        if !is_open {
            return Err(ApplicationError::validation("Market is closed"));
        }

        // Convert DTO to domain types
        let symbol =
            Symbol::new(&dto.symbol).map_err(|e| ApplicationError::validation(e.to_string()))?;
        let quantity =
            Quantity::new(dto.quantity).map_err(|e| ApplicationError::validation(e.to_string()))?;
        let side = dto.side.into();

        // Create order using domain service
        let (order, _event) = self
            .order_service
            .create_order(dto.account_id, symbol, side, quantity)
            .map_err(ApplicationError::from)?;

        // Persist order
        self.order_repo
            .save(&order)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?;

        // Submit to market
        self.execution_port
            .submit_order(order.id())
            .await
            .map_err(|e| ApplicationError::infrastructure(e.to_string()))?;

        // Notify
        self.notification_port
            .notify(
                &format!("Order {} created for {}", order.id(), dto.symbol),
                NotificationLevel::Info,
            )
            .await
            .map_err(|e| ApplicationError::infrastructure(e.to_string()))?;

        info!(
            order_id = %order.id(),
            symbol = %dto.symbol,
            "Order created successfully"
        );

        Ok(order_to_dto(&order))
    }
}

/// Use case for filling orders
pub struct FillOrderUseCase {
    order_service: OrderService,
    order_repo: Arc<dyn OrderRepository>,
    notification_port: Arc<dyn NotificationPort>,
}

impl FillOrderUseCase {
    /// Creates a new FillOrderUseCase
    #[must_use]
    pub fn new(
        order_repo: Arc<dyn OrderRepository>,
        notification_port: Arc<dyn NotificationPort>,
    ) -> Self {
        Self {
            order_service: OrderService::new(),
            order_repo,
            notification_port,
        }
    }

    /// Executes the use case
    ///
    /// # Errors
    ///
    /// Returns error if order fill fails
    pub async fn execute(&self, dto: OrderFillDto) -> ApplicationResult<OrderDto> {
        // Find order
        let mut order = self
            .order_repo
            .find_by_id(dto.order_id)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?
            .ok_or_else(|| ApplicationError::not_found(format!("Order {}", dto.order_id)))?;

        // Validate fill quantity
        let fill_qty = Quantity::new(dto.fill_quantity)
            .map_err(|e| ApplicationError::validation(e.to_string()))?;
        let fill_price =
            Price::new(dto.fill_price).map_err(|e| ApplicationError::validation(e.to_string()))?;

        // Apply fill using domain service
        let _event = self
            .order_service
            .fill_order(&mut order, fill_qty, fill_price)
            .map_err(ApplicationError::from)?;

        // Persist updated order
        self.order_repo
            .save(&order)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?;

        // Notify
        let level = if matches!(order.status(), domain::OrderStatus::Filled { .. }) {
            NotificationLevel::Info
        } else {
            NotificationLevel::Info
        };

        self.notification_port
            .notify(
                &format!(
                    "Order {} filled: {} @ {}",
                    order.id(),
                    dto.fill_quantity,
                    dto.fill_price
                ),
                level,
            )
            .await
            .map_err(|e| ApplicationError::infrastructure(e.to_string()))?;

        info!(
            order_id = %order.id(),
            fill_qty = %dto.fill_quantity,
            fill_price = %dto.fill_price,
            "Order filled"
        );

        Ok(order_to_dto(&order))
    }
}

/// Use case for cancelling orders
pub struct CancelOrderUseCase {
    order_service: OrderService,
    order_repo: Arc<dyn OrderRepository>,
    execution_port: Arc<dyn OrderExecutionPort>,
    notification_port: Arc<dyn NotificationPort>,
}

impl CancelOrderUseCase {
    /// Creates a new CancelOrderUseCase
    #[must_use]
    pub fn new(
        order_repo: Arc<dyn OrderRepository>,
        execution_port: Arc<dyn OrderExecutionPort>,
        notification_port: Arc<dyn NotificationPort>,
    ) -> Self {
        Self {
            order_service: OrderService::new(),
            order_repo,
            execution_port,
            notification_port,
        }
    }

    /// Executes the use case
    ///
    /// # Errors
    ///
    /// Returns error if order cancellation fails
    pub async fn execute(
        &self,
        order_id: EntityId,
        reason: Option<String>,
    ) -> ApplicationResult<OrderDto> {
        // Find order
        let mut order = self
            .order_repo
            .find_by_id(order_id)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?
            .ok_or_else(|| ApplicationError::not_found(format!("Order {}", order_id)))?;

        // Check if order can be cancelled
        if !order.is_active() {
            return Err(ApplicationError::validation(
                "Order is not active and cannot be cancelled",
            ));
        }

        // Cancel in market first
        self.execution_port
            .cancel_order(order_id)
            .await
            .map_err(|e| ApplicationError::infrastructure(e.to_string()))?;

        // Apply cancellation using domain service
        let _event = self
            .order_service
            .cancel_order(&mut order, reason.clone())
            .map_err(ApplicationError::from)?;

        // Persist updated order
        self.order_repo
            .save(&order)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?;

        // Notify
        self.notification_port
            .notify(
                &format!("Order {} cancelled: {:?}", order_id, reason),
                NotificationLevel::Warning,
            )
            .await
            .map_err(|e| ApplicationError::infrastructure(e.to_string()))?;

        warn!(
            order_id = %order_id,
            reason = ?reason,
            "Order cancelled"
        );

        Ok(order_to_dto(&order))
    }
}

/// Converts a domain Order to OrderDto
fn order_to_dto(order: &domain::Order) -> OrderDto {
    OrderDto {
        id: order.id(),
        account_id: order.account_id(),
        symbol: order.symbol().as_str().to_string(),
        side: order.side().into(),
        order_type: order.order_type().into(),
        quantity: order.quantity().inner(),
        filled_quantity: order.filled_quantity().inner(),
        limit_price: order.limit_price().map(|p| p.inner()),
        stop_price: order.stop_price().map(|p| p.inner()),
        status: order.status().into(),
        created_at: order.created_at(),
        updated_at: order.updated_at(),
    }
}
