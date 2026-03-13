//! # Position Use Cases
//!
//! Use cases for position operations.

use std::sync::Arc;

use tracing::{info, warn};

use crate::dto::{PositionDirectionDto, PositionDto};
use crate::errors::{ApplicationError, ApplicationResult};
use crate::ports::{MarketDataPort, NotificationLevel, NotificationPort};
use domain::{
    Currency, EntityId, OrderSide, PositionRepository, PositionService, Price, Quantity, Symbol,
};

/// Use case for retrieving positions
pub struct GetPositionsUseCase {
    position_repo: Arc<dyn PositionRepository>,
    market_data_port: Arc<dyn MarketDataPort>,
}

impl GetPositionsUseCase {
    /// Creates a new GetPositionsUseCase
    #[must_use]
    pub fn new(
        position_repo: Arc<dyn PositionRepository>,
        market_data_port: Arc<dyn MarketDataPort>,
    ) -> Self {
        Self {
            position_repo,
            market_data_port,
        }
    }

    /// Gets all positions for an account
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub async fn get_by_account(
        &self,
        account_id: EntityId,
    ) -> ApplicationResult<Vec<PositionDto>> {
        let positions = self
            .position_repo
            .find_by_account(account_id)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?;

        let mut dtos = Vec::new();
        for pos in &positions {
            let mut dto = position_to_dto(pos);

            // Enrich with current market price
            if let Ok(price) = self.market_data_port.get_current_price(&dto.symbol).await {
                dto.current_price = Some(price);
            }

            dtos.push(dto);
        }

        Ok(dtos)
    }

    /// Gets a position by ID
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub async fn get_by_id(&self, position_id: EntityId) -> ApplicationResult<PositionDto> {
        let position = self
            .position_repo
            .find_by_id(position_id)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?
            .ok_or_else(|| ApplicationError::not_found(format!("Position {}", position_id)))?;

        let mut dto = position_to_dto(&position);

        // Enrich with current market price
        if let Ok(price) = self.market_data_port.get_current_price(&dto.symbol).await {
            dto.current_price = Some(price);
        }

        Ok(dto)
    }

    /// Gets all open positions
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub async fn get_open_positions(&self) -> ApplicationResult<Vec<PositionDto>> {
        let positions = self
            .position_repo
            .find_open_positions()
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?;

        let mut dtos = Vec::new();
        for pos in &positions {
            let mut dto = position_to_dto(pos);

            // Enrich with current market price
            if let Ok(price) = self.market_data_port.get_current_price(&dto.symbol).await {
                dto.current_price = Some(price);
            }

            dtos.push(dto);
        }

        Ok(dtos)
    }
}

/// Use case for closing positions
pub struct ClosePositionUseCase {
    position_service: PositionService,
    position_repo: Arc<dyn PositionRepository>,
    market_data_port: Arc<dyn MarketDataPort>,
    notification_port: Arc<dyn NotificationPort>,
}

impl ClosePositionUseCase {
    /// Creates a new ClosePositionUseCase
    #[must_use]
    pub fn new(
        position_repo: Arc<dyn PositionRepository>,
        market_data_port: Arc<dyn MarketDataPort>,
        notification_port: Arc<dyn NotificationPort>,
    ) -> Self {
        Self {
            position_service: PositionService::new(),
            position_repo,
            market_data_port,
            notification_port,
        }
    }

    /// Closes a position (full or partial)
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub async fn execute(
        &self,
        position_id: EntityId,
        close_qty: Option<Quantity>,
    ) -> ApplicationResult<PositionDto> {
        // Find position
        let mut position = self
            .position_repo
            .find_by_id(position_id)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?
            .ok_or_else(|| ApplicationError::not_found(format!("Position {}", position_id)))?;

        // Determine quantity to close
        let quantity_to_close = close_qty.unwrap_or_else(|| position.quantity());

        // Get current market price
        let symbol = position.symbol().as_str();
        let current_price = self
            .market_data_port
            .get_current_price(symbol)
            .await
            .map_err(|e| ApplicationError::infrastructure(e.to_string()))?;
        let exit_price =
            Price::new(current_price).map_err(|e| ApplicationError::validation(e.to_string()))?;

        // Close position using domain service
        let (closed_event, _updated_event) = self
            .position_service
            .close_position(&mut position, quantity_to_close, exit_price)
            .map_err(ApplicationError::from)?;

        // Persist updated position
        self.position_repo
            .save(&position)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?;

        // Notify
        let level = if closed_event.is_some() {
            NotificationLevel::Info
        } else {
            NotificationLevel::Info
        };

        self.notification_port
            .notify(
                &format!(
                    "Position {} closed: {} shares @ {}",
                    position_id,
                    quantity_to_close.inner(),
                    current_price
                ),
                level,
            )
            .await
            .map_err(|e| ApplicationError::infrastructure(e.to_string()))?;

        if closed_event.is_some() {
            info!(
                position_id = %position_id,
                realized_pnl = %position.realized_pnl().amount(),
                "Position fully closed"
            );
        } else {
            warn!(
                position_id = %position_id,
                remaining = %(position.quantity().inner() - quantity_to_close.inner()),
                "Position partially closed"
            );
        }

        Ok(position_to_dto(&position))
    }
}

/// Converts a domain Position to PositionDto
fn position_to_dto(position: &domain::Position) -> PositionDto {
    PositionDto {
        id: position.id(),
        account_id: position.account_id(),
        symbol: position.symbol().as_str().to_string(),
        direction: position.direction().into(),
        quantity: position.quantity().inner(),
        avg_entry_price: position.avg_entry_price().inner(),
        current_price: position.current_price().map(|p| p.inner()),
        unrealized_pnl: position.unrealized_pnl().map(|m| m.amount()),
        realized_pnl: position.realized_pnl().amount(),
        opened_at: position.opened_at(),
    }
}
