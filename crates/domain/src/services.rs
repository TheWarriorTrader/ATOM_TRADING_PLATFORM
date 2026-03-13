//! # Domain Services
//!
//! Business logic and domain operations that don't naturally fit within entities.

use tracing::info;

use crate::entities::{
    Account, EntityId, Order, OrderSide, OrderStatus, Position, PositionDirection,
};
use crate::errors::{DomainError, DomainResult};
use crate::events::{
    OrderCancelled, OrderCreated, OrderFilled, PositionClosed, PositionOpened, PositionUpdated,
};
use crate::values::{Currency, Money, Price, Quantity, Symbol};

/// Service for managing order lifecycle
#[derive(Debug, Clone, Default)]
pub struct OrderService;

impl OrderService {
    /// Creates a new OrderService
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Creates a new order for an account
    ///
    /// # Errors
    ///
    /// Returns error if validation fails
    pub fn create_order(
        &self,
        account_id: EntityId,
        symbol: Symbol,
        side: OrderSide,
        quantity: Quantity,
    ) -> DomainResult<(Order, OrderCreated)> {
        let order = Order::new(
            account_id,
            symbol.clone(),
            side,
            crate::entities::OrderType::Market,
            quantity,
            None,
            None,
        )?;

        let event = OrderCreated::new(order.id(), account_id, symbol.clone(), side, quantity);

        info!(
            order_id = %order.id(),
            account_id = %account_id,
            symbol = %symbol,
            "Order created"
        );

        Ok((order, event))
    }

    /// Cancels an order
    ///
    /// # Errors
    ///
    /// Returns error if the order cannot be cancelled
    pub fn cancel_order(
        &self,
        order: &mut Order,
        reason: impl Into<Option<String>>,
    ) -> DomainResult<OrderCancelled> {
        order.update_status(OrderStatus::Cancelled)?;

        let event = OrderCancelled::new(order.id(), order.account_id(), reason);

        info!(
            order_id = %order.id(),
            "Order cancelled"
        );

        Ok(event)
    }

    /// Fills a portion of an order
    ///
    /// # Errors
    ///
    /// Returns error if the fill is invalid
    pub fn fill_order(
        &self,
        order: &mut Order,
        fill_qty: Quantity,
        fill_price: Price,
    ) -> DomainResult<OrderFilled> {
        order.fill(fill_qty)?;

        let event = OrderFilled::new(
            order.id(),
            order.account_id(),
            order.symbol().clone(),
            fill_qty,
            fill_price,
            order.filled_quantity(),
            order.status() == OrderStatus::Filled,
        );

        info!(
            order_id = %order.id(),
            fill_qty = %fill_qty,
            fill_price = %fill_price,
            "Order filled"
        );

        Ok(event)
    }
}

/// Service for managing positions
#[derive(Debug, Clone, Default)]
pub struct PositionService;

impl PositionService {
    /// Creates a new PositionService
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Opens a new position from an order fill
    ///
    /// # Errors
    ///
    /// Returns error if the position cannot be opened
    pub fn open_position(
        &self,
        account_id: EntityId,
        symbol: Symbol,
        side: OrderSide,
        quantity: Quantity,
        entry_price: Price,
        currency: Currency,
    ) -> DomainResult<(Position, PositionOpened)> {
        let direction = match side {
            OrderSide::Buy => PositionDirection::Long,
            OrderSide::Sell => PositionDirection::Short,
        };

        let position = Position::new(
            account_id,
            symbol.clone(),
            direction,
            quantity,
            entry_price,
            currency,
        );

        let event = PositionOpened::new(
            position.id(),
            account_id,
            symbol,
            direction,
            quantity,
            entry_price,
        );

        info!(
            position_id = %position.id(),
            account_id = %account_id,
            symbol = %position.symbol(),
            "Position opened"
        );

        Ok((position, event))
    }

    /// Increases an existing position
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub fn increase_position(
        &self,
        position: &mut Position,
        additional_qty: Quantity,
        price: Price,
    ) -> DomainResult<PositionUpdated> {
        // Calculate new average entry price
        let current_qty = position.quantity().inner();
        let new_qty = current_qty + additional_qty.inner();
        let avg_price = (position.avg_entry_price().inner() * current_qty
            + price.inner() * additional_qty.inner())
            / new_qty;

        // Update position
        let new_quantity = unsafe { Quantity::new_unchecked(new_qty) };
        let new_avg_price = unsafe { Price::new_unchecked(avg_price) };

        let event = PositionUpdated::new(
            position.id(),
            position.account_id(),
            new_quantity,
            new_avg_price,
        );

        info!(
            position_id = %position.id(),
            new_quantity = %new_quantity,
            "Position increased"
        );

        Ok(event)
    }

    /// Closes a position (full or partial)
    ///
    /// # Errors
    ///
    /// Returns error if the close quantity exceeds position size
    pub fn close_position(
        &self,
        position: &mut Position,
        close_qty: Quantity,
        exit_price: Price,
    ) -> DomainResult<(Option<PositionClosed>, PositionUpdated)> {
        if close_qty.inner() > position.quantity().inner() {
            return Err(DomainError::invalid_quantity(
                "Close quantity exceeds position size",
            ));
        }

        // Calculate realized P&L
        let pnl_per_unit = match position.direction() {
            PositionDirection::Long => exit_price.inner() - position.avg_entry_price().inner(),
            PositionDirection::Short => position.avg_entry_price().inner() - exit_price.inner(),
        };
        let realized_pnl = pnl_per_unit * close_qty.inner();

        let currency = position.realized_pnl().currency();
        position.add_realized_pnl(unsafe { Money::new_unchecked(realized_pnl, currency) });

        let new_qty = position.quantity().inner() - close_qty.inner();

        if new_qty == rust_decimal::Decimal::ZERO {
            // Full close
            let event = PositionClosed::new(
                position.id(),
                position.account_id(),
                position.symbol().clone(),
                close_qty,
                exit_price,
                realized_pnl,
            );

            info!(
                position_id = %position.id(),
                realized_pnl = %realized_pnl,
                "Position fully closed"
            );

            Ok((
                Some(event),
                PositionUpdated::new(
                    position.id(),
                    position.account_id(),
                    unsafe { Quantity::new_unchecked(rust_decimal::Decimal::ZERO) },
                    position.avg_entry_price(),
                ),
            ))
        } else {
            // Partial close
            let updated = PositionUpdated::new(
                position.id(),
                position.account_id(),
                unsafe { Quantity::new_unchecked(new_qty) },
                position.avg_entry_price(),
            );

            info!(
                position_id = %position.id(),
                remaining_qty = %new_qty,
                realized_pnl = %realized_pnl,
                "Position partially closed"
            );

            Ok((None, updated))
        }
    }
}

/// Service for managing account operations
#[derive(Debug, Clone, Default)]
pub struct AccountService;

impl AccountService {
    /// Creates a new AccountService
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Creates a new trading account
    ///
    /// # Errors
    ///
    /// Returns error if validation fails
    pub fn create_account(
        &self,
        name: impl Into<String>,
        currency: Currency,
        initial_balance: rust_decimal::Decimal,
    ) -> DomainResult<Account> {
        Account::new(name, currency, initial_balance)
    }

    /// Updates account balance
    pub fn update_balance(&self, account: &mut Account, new_balance: rust_decimal::Decimal) {
        account.update_balance(new_balance);

        info!(
            account_id = %account.id(),
            new_balance = %new_balance,
            "Account balance updated"
        );
    }

    /// Checks if account has sufficient funds
    #[must_use]
    pub fn has_sufficient_funds(&self, account: &Account, required: rust_decimal::Decimal) -> bool {
        account.balance().amount() >= required
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_order_service_create_order() {
        let service = OrderService::new();
        let account_id = EntityId::new_v4();
        let symbol = Symbol::new("AAPL").unwrap();
        let quantity = Quantity::new(dec!(100)).unwrap();

        let (order, event) = service
            .create_order(account_id, symbol.clone(), OrderSide::Buy, quantity)
            .unwrap();

        assert_eq!(order.account_id(), account_id);
        assert_eq!(order.symbol(), &symbol);
        assert_eq!(order.side(), OrderSide::Buy);
        assert_eq!(event.symbol, symbol);
    }

    #[test]
    fn test_position_service_open_position() {
        let service = PositionService::new();
        let account_id = EntityId::new_v4();
        let symbol = Symbol::new("AAPL").unwrap();
        let quantity = Quantity::new(dec!(100)).unwrap();
        let price = Price::new(dec!(150.0)).unwrap();

        let (position, event) = service
            .open_position(
                account_id,
                symbol.clone(),
                OrderSide::Buy,
                quantity,
                price,
                Currency::USD,
            )
            .unwrap();

        assert_eq!(position.account_id(), account_id);
        assert_eq!(position.symbol(), &symbol);
        assert_eq!(position.quantity(), quantity);
        assert_eq!(event.symbol, symbol);
    }
}
