//! # Repository Traits
//!
//! Abstract repository interfaces for persistence operations.
//! These traits define the contract that infrastructure implementations must fulfill.

use async_trait::async_trait;

use crate::entities::{Account, EntityId, Order, Position};
use crate::errors::DomainResult;
use crate::values::Symbol;

/// Repository for Account entities
#[async_trait]
pub trait AccountRepository: Send + Sync {
    /// Finds an account by ID
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn find_by_id(&self, id: EntityId) -> DomainResult<Option<Account>>;

    /// Finds all accounts
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn find_all(&self) -> DomainResult<Vec<Account>>;

    /// Saves an account
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn save(&self, account: &Account) -> DomainResult<()>;

    /// Deletes an account by ID
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn delete(&self, id: EntityId) -> DomainResult<()>;
}

/// Repository for Order entities
#[async_trait]
pub trait OrderRepository: Send + Sync {
    /// Finds an order by ID
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn find_by_id(&self, id: EntityId) -> DomainResult<Option<Order>>;

    /// Finds all orders for an account
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn find_by_account(&self, account_id: EntityId) -> DomainResult<Vec<Order>>;

    /// Finds all active orders for an account
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn find_active_by_account(&self, account_id: EntityId) -> DomainResult<Vec<Order>>;

    /// Saves an order
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn save(&self, order: &Order) -> DomainResult<()>;

    /// Deletes an order by ID
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn delete(&self, id: EntityId) -> DomainResult<()>;
}

/// Repository for Position entities
#[async_trait]
pub trait PositionRepository: Send + Sync {
    /// Finds a position by ID
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn find_by_id(&self, id: EntityId) -> DomainResult<Option<Position>>;

    /// Finds all positions for an account
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn find_by_account(&self, account_id: EntityId) -> DomainResult<Vec<Position>>;

    /// Finds a position by account and symbol
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn find_by_account_and_symbol(
        &self,
        account_id: EntityId,
        symbol: &Symbol,
    ) -> DomainResult<Option<Position>>;

    /// Finds all open positions
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn find_open_positions(&self) -> DomainResult<Vec<Position>>;

    /// Saves a position
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn save(&self, position: &Position) -> DomainResult<()>;

    /// Deletes a position by ID
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    async fn delete(&self, id: EntityId) -> DomainResult<()>;
}

/// Unit of work for transactional operations
#[async_trait]
pub trait UnitOfWork: Send + Sync {
    /// Commits all pending changes
    ///
    /// # Errors
    ///
    /// Returns error if the commit fails
    async fn commit(&mut self) -> DomainResult<()>;

    /// Rolls back all pending changes
    ///
    /// # Errors
    ///
    /// Returns error if the rollback fails
    async fn rollback(&mut self) -> DomainResult<()>;
}
