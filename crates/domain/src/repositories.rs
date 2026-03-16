//! # Repository Traits
//!
//! Abstract repository interfaces for persistence operations.
//! These traits define the contract that infrastructure implementations must fulfill.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::entities::{Account, EntityId, Fill, Order, OrderStatus, Position};
use crate::errors::{DomainResult, RepositoryError};
use crate::values::{Money, OrderId, Side, Symbol};

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

    /// Updates an existing order (UPSERT semantics).
    ///
    /// If the order exists, it will be updated. If not, this operation
    /// may return an error depending on implementation.
    ///
    /// # Arguments
    ///
    /// * `order` - The order to update
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::NotFound`] if the order does not exist.
    /// Returns [`RepositoryError::QueryFailed`] if the database operation fails.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::repositories::OrderRepository;
    ///
    /// async fn update_order(repo: &dyn OrderRepository, order: &Order) -> Result<(), RepositoryError> {
    ///     repo.update(order).await
    /// }
    /// ```
    async fn update(&self, order: &Order) -> Result<(), RepositoryError>;

    /// Retrieves an order by its OrderId.
    ///
    /// # Arguments
    ///
    /// * `order_id` - The unique order identifier
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database operation fails.
    async fn get(&self, order_id: OrderId) -> Result<Option<Order>, RepositoryError>;

    /// Retrieves all open (active) orders.
    ///
    /// Open orders are those that can still be filled:
    /// Created, Submitted, Pending, or PartiallyFilled.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database operation fails.
    async fn get_open(&self) -> Result<Vec<Order>, RepositoryError>;

    /// Retrieves orders for a specific symbol.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol to filter by
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database operation fails.
    async fn get_by_symbol(&self, symbol: &Symbol) -> Result<Vec<Order>, RepositoryError>;

    /// Retrieves order history within a time range.
    ///
    /// # Arguments
    ///
    /// * `start` - Start of the time range (inclusive)
    /// * `end` - End of the time range (inclusive)
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database operation fails.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use chrono::{Duration, Utc};
    /// use domain::repositories::OrderRepository;
    ///
    /// async fn get_recent_orders(repo: &dyn OrderRepository) -> Result<Vec<Order>, RepositoryError> {
    ///     let end = Utc::now();
    ///     let start = end - Duration::days(7);
    ///     repo.get_history(start, end).await
    /// }
    /// ```
    async fn get_history(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Order>, RepositoryError>;

    /// Counts orders by their status.
    ///
    /// # Arguments
    ///
    /// * `status` - The order status to count
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database operation fails.
    async fn count_by_status(&self, status: OrderStatus) -> Result<u64, RepositoryError>;
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

    /// Saves or updates a position (UPSERT semantics).
    ///
    /// If a position for the symbol exists, it will be updated.
    /// If not, a new position will be created.
    ///
    /// # Arguments
    ///
    /// * `position` - The position to save or update
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::ConnectionFailed`] if database connection fails.
    /// Returns [`RepositoryError::QueryFailed`] if the upsert operation fails.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::repositories::PositionRepository;
    ///
    /// async fn upsert_position(repo: &dyn PositionRepository, position: &Position) -> Result<(), RepositoryError> {
    ///     repo.upsert(position).await
    /// }
    /// ```
    async fn upsert(&self, position: &Position) -> Result<(), RepositoryError>;

    /// Retrieves a position by symbol.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database query fails.
    async fn get_by_symbol(&self, symbol: &Symbol) -> Result<Option<Position>, RepositoryError>;

    /// Retrieves all open positions.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database query fails.
    async fn get_all_open(&self) -> Result<Vec<Position>, RepositoryError>;

    /// Retrieves positions by side (Long/Short).
    ///
    /// # Arguments
    ///
    /// * `side` - The position side (Buy=Long, Sell=Short)
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database query fails.
    async fn get_by_side(&self, side: Side) -> Result<Vec<Position>, RepositoryError>;

    /// Closes a position (logical deletion).
    ///
    /// The position is marked as closed rather than physically deleted
    /// to maintain historical records for audit purposes.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol of the position to close
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::NotFound`] if no position exists for the symbol.
    /// Returns [`RepositoryError::QueryFailed`] if the close operation fails.
    async fn close(&self, symbol: &Symbol) -> Result<(), RepositoryError>;

    /// Checks if a position exists for a symbol.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol to check
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database query fails.
    async fn exists(&self, symbol: &Symbol) -> Result<bool, RepositoryError>;

    /// Calculates the total unrealized P&L across all positions.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the calculation fails.
    /// Returns [`RepositoryError`] if currency conversion is needed but fails.
    async fn total_pnl(&self) -> Result<Money, RepositoryError>;

    /// Calculates the total exposure across all positions.
    ///
    /// Exposure is the absolute value of position values.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the calculation fails.
    /// Returns [`RepositoryError`] if currency conversion is needed but fails.
    async fn total_exposure(&self) -> Result<Money, RepositoryError>;
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

// =============================================================================
// EXECUTION REPOSITORIES
// =============================================================================

/// Repository for Fill (execution) entities.
///
/// This trait defines the contract for persisting and retrieving fill data.
/// Fill records represent executed trades/orders and are critical for
/// position tracking and P&L calculations.
///
/// # Design Notes
///
/// - Batch operations are provided for high-frequency trading scenarios
/// - Time-range queries support trade reconciliation and reporting
/// - All operations return `Result<T, RepositoryError>` for explicit error handling
#[async_trait]
pub trait FillRepository: Send + Sync {
    /// Saves a single fill to the repository.
    ///
    /// # Arguments
    ///
    /// * `fill` - The fill to save
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::ConnectionFailed`] if database connection fails.
    /// Returns [`RepositoryError::DuplicateKey`] if fill already exists.
    /// Returns [`RepositoryError::QueryFailed`] if the insert operation fails.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::repositories::FillRepository;
    ///
    /// async fn save_fill(repo: &dyn FillRepository, fill: &Fill) -> Result<(), RepositoryError> {
    ///     repo.save(fill).await
    /// }
    /// ```
    async fn save(&self, fill: &Fill) -> Result<(), RepositoryError>;

    /// Saves multiple fills in a batch operation.
    ///
    /// This is significantly more efficient than calling [`save`] multiple times
    /// for high-frequency trading scenarios where multiple fills arrive simultaneously.
    ///
    /// # Arguments
    ///
    /// * `fills` - Slice of fills to save
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::ConnectionFailed`] if database connection fails.
    /// Returns [`RepositoryError::DuplicateKey`] if any fill already exists.
    /// Returns [`RepositoryError::QueryFailed`] if the batch insert fails.
    ///
    /// # Performance
    ///
    /// Implementations should use bulk insert operations (e.g., COPY in PostgreSQL)
    /// for optimal performance with large batches.
    async fn save_batch(&self, fills: &[Fill]) -> Result<(), RepositoryError>;

    /// Retrieves all fills for a specific order.
    ///
    /// # Arguments
    ///
    /// * `order_id` - The order ID to query
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database query fails.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::repositories::FillRepository;
    /// use domain::values::OrderId;
    ///
    /// async fn get_order_fills(repo: &dyn FillRepository, order_id: OrderId) -> Result<Vec<Fill>, RepositoryError> {
    ///     repo.get_by_order(order_id).await
    /// }
    /// ```
    async fn get_by_order(&self, order_id: OrderId) -> Result<Vec<Fill>, RepositoryError>;

    /// Retrieves fills for a symbol within a time range.
    ///
    /// Results are ordered by timestamp in ascending order.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol
    /// * `start` - Start of the time range (inclusive)
    /// * `end` - End of the time range (inclusive)
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database query fails.
    ///
    /// # Performance Considerations
    ///
    /// Fill data can be dense during high-volume periods. Consider:
    /// - Using time-based partitioning in storage
    /// - Implementing query limits for large ranges
    async fn get_by_symbol_range(
        &self,
        symbol: &Symbol,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Fill>, RepositoryError>;

    /// Retrieves the most recent N fills.
    ///
    /// Results are ordered by timestamp in descending order (newest first).
    ///
    /// # Arguments
    ///
    /// * `n` - Maximum number of fills to retrieve
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::QueryFailed`] if the database query fails.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::repositories::FillRepository;
    ///
    /// async fn get_latest_fills(repo: &dyn FillRepository) -> Result<Vec<Fill>, RepositoryError> {
    ///     repo.get_recent(100).await
    /// }
    /// ```
    async fn get_recent(&self, n: usize) -> Result<Vec<Fill>, RepositoryError>;
}

// =============================================================================
// MARKET DATA REPOSITORIES
// =============================================================================

use crate::entities::{Bar, Tick};

/// Repository for OHLCV Bar entities.
///
/// This trait defines the contract for persisting and retrieving bar data.
/// Implementations are responsible for efficient storage and retrieval
/// of historical market data.
///
/// # Design Notes
///
/// - All operations are async to support non-blocking I/O
/// - Batch operations are preferred for bulk inserts
/// - Time-range queries should be optimized for backtesting scenarios
#[async_trait]
pub trait BarRepository: Send + Sync {
    /// Saves a single bar to the repository.
    ///
    /// # Arguments
    ///
    /// * `bar` - The bar to save
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if:
    /// - Database connection fails
    /// - Serialization fails
    /// - Constraint violation occurs (e.g., duplicate key)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::repositories::BarRepository;
    ///
    /// async fn save_bar(repo: &dyn BarRepository, bar: &Bar) -> DomainResult<()> {
    ///     repo.save(bar).await
    /// }
    /// ```
    async fn save(&self, bar: &Bar) -> DomainResult<()>;

    /// Saves multiple bars in a single batch operation.
    ///
    /// This is more efficient than calling [`save`] multiple times
    /// as it minimizes database round-trips.
    ///
    /// # Arguments
    ///
    /// * `bars` - Slice of bars to save
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if:
    /// - Database connection fails
    /// - Any bar fails validation or serialization
    /// - Constraint violation occurs
    ///
    /// # Performance
    ///
    /// Implementations should use bulk insert operations where available.
    async fn save_batch(&self, bars: &[Bar]) -> DomainResult<()>;

    /// Retrieves bars for a symbol within a time range.
    ///
    /// Results are ordered by timestamp in ascending order.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol
    /// * `start` - Start of the time range (inclusive)
    /// * `end` - End of the time range (inclusive)
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if:
    /// - Database query fails
    /// - Invalid time range is provided
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use chrono::{Duration, Utc};
    /// use domain::repositories::BarRepository;
    /// use domain::values::Symbol;
    ///
    /// async fn get_daily_bars(repo: &dyn BarRepository) -> DomainResult<Vec<Bar>> {
    ///     let symbol = Symbol::new("AAPL").unwrap();
    ///     let end = Utc::now();
    ///     let start = end - Duration::days(30);
    ///     repo.get_range(&symbol, start, end).await
    /// }
    /// ```
    async fn get_range(
        &self,
        symbol: &Symbol,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DomainResult<Vec<Bar>>;

    /// Retrieves the latest N bars for a symbol.
    ///
    /// Results are ordered by timestamp in descending order (newest first).
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol
    /// * `n` - Maximum number of bars to retrieve
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if:
    /// - Database query fails
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::repositories::BarRepository;
    /// use domain::values::Symbol;
    ///
    /// async fn get_latest_10(repo: &dyn BarRepository) -> DomainResult<Vec<Bar>> {
    ///     let symbol = Symbol::new("AAPL").unwrap();
    ///     repo.get_latest(&symbol, 10).await
    /// }
    /// ```
    async fn get_latest(&self, symbol: &Symbol, n: usize) -> DomainResult<Vec<Bar>>;
}

/// Repository for Tick entities.
///
/// This trait defines the contract for persisting and retrieving tick data.
/// Tick data represents individual trades or quotes and is the most
/// granular form of market data available.
///
/// # Design Notes
///
/// - Tick data volumes can be extremely high, so queries should be time-bounded
/// - Storage implementations may use time-series optimized databases
#[async_trait]
pub trait TickRepository: Send + Sync {
    /// Saves a single tick to the repository.
    ///
    /// # Arguments
    ///
    /// * `tick` - The tick to save
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if:
    /// - Database connection fails
    /// - Serialization fails
    /// - Constraint violation occurs
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::repositories::TickRepository;
    ///
    /// async fn save_tick(repo: &dyn TickRepository, tick: &Tick) -> DomainResult<()> {
    ///     repo.save(tick).await
    /// }
    /// ```
    async fn save(&self, tick: &Tick) -> DomainResult<()>;

    /// Retrieves ticks for a symbol within a time range.
    ///
    /// Results are ordered by timestamp in ascending order.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol
    /// * `start` - Start of the time range (inclusive)
    /// * `end` - End of the time range (inclusive)
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if:
    /// - Database query fails
    /// - Invalid time range is provided
    ///
    /// # Performance Considerations
    ///
    /// Tick data can be very dense. Consider:
    /// - Using pagination for large time ranges
    /// - Implementing time-based partitioning in storage
    /// - Setting reasonable query limits
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use chrono::{Duration, Utc};
    /// use domain::repositories::TickRepository;
    /// use domain::values::Symbol;
    ///
    /// async fn get_minute_ticks(repo: &dyn TickRepository) -> DomainResult<Vec<Tick>> {
    ///     let symbol = Symbol::new("AAPL").unwrap();
    ///     let end = Utc::now();
    ///     let start = end - Duration::minutes(1);
    ///     repo.get_range(&symbol, start, end).await
    /// }
    /// ```
    async fn get_range(
        &self,
        symbol: &Symbol,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DomainResult<Vec<Tick>>;

    /// Saves multiple ticks in a batch operation.
    ///
    /// This is more efficient than calling [`save`] multiple times
    /// as it minimizes database round-trips.
    ///
    /// # Arguments
    ///
    /// * `ticks` - Slice of ticks to save
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if:
    /// - Database connection fails
    /// - Any tick fails validation or serialization
    /// - Constraint violation occurs
    ///
    /// # Performance
    ///
    /// Implementations should use bulk insert operations where available.
    async fn save_batch(&self, ticks: &[Tick]) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    // =============================================================================
    // MOCK ORDER REPOSITORY
    // =============================================================================

    /// Mock implementation of OrderRepository for testing
    #[derive(Debug, Clone)]
    struct MockOrderRepository {
        orders: Arc<Mutex<HashMap<OrderId, Order>>>,
    }

    impl MockOrderRepository {
        fn new() -> Self {
            Self {
                orders: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl OrderRepository for MockOrderRepository {
        async fn find_by_id(&self, id: EntityId) -> DomainResult<Option<Order>> {
            let orders = self.orders.lock().unwrap();
            // Search by EntityId - for mock we return None as OrderId != EntityId
            Ok(None)
        }

        async fn find_by_account(&self, _account_id: EntityId) -> DomainResult<Vec<Order>> {
            Ok(Vec::new())
        }

        async fn find_active_by_account(&self, _account_id: EntityId) -> DomainResult<Vec<Order>> {
            Ok(Vec::new())
        }

        async fn save(&self, order: &Order) -> DomainResult<()> {
            // Mock implementation does nothing for basic save
            Ok(())
        }

        async fn delete(&self, _id: EntityId) -> DomainResult<()> {
            Ok(())
        }

        async fn update(&self, _order: &Order) -> Result<(), RepositoryError> {
            Ok(())
        }

        async fn get(&self, order_id: OrderId) -> Result<Option<Order>, RepositoryError> {
            let orders = self.orders.lock().unwrap();
            Ok(orders.get(&order_id).cloned())
        }

        async fn get_open(&self) -> Result<Vec<Order>, RepositoryError> {
            let orders = self.orders.lock().unwrap();
            Ok(orders
                .values()
                .filter(|o| o.status().is_active())
                .cloned()
                .collect())
        }

        async fn get_by_symbol(&self, symbol: &Symbol) -> Result<Vec<Order>, RepositoryError> {
            let orders = self.orders.lock().unwrap();
            Ok(orders
                .values()
                .filter(|o| o.symbol() == symbol)
                .cloned()
                .collect())
        }

        async fn get_history(
            &self,
            start: DateTime<Utc>,
            end: DateTime<Utc>,
        ) -> Result<Vec<Order>, RepositoryError> {
            let orders = self.orders.lock().unwrap();
            Ok(orders
                .values()
                .filter(|o| {
                    let created_at = o.created_at();
                    created_at >= start && created_at <= end
                })
                .cloned()
                .collect())
        }

        async fn count_by_status(&self, status: OrderStatus) -> Result<u64, RepositoryError> {
            let orders = self.orders.lock().unwrap();
            let count = orders.values().filter(|o| o.status() == status).count();
            Ok(count as u64)
        }
    }

    // =============================================================================
    // MOCK FILL REPOSITORY
    // =============================================================================

    /// Mock implementation of FillRepository for testing
    #[derive(Debug, Clone)]
    struct MockFillRepository {
        fills: Arc<Mutex<Vec<Fill>>>,
    }

    impl MockFillRepository {
        fn new() -> Self {
            Self {
                fills: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl FillRepository for MockFillRepository {
        async fn save(&self, fill: &Fill) -> Result<(), RepositoryError> {
            let mut fills = self.fills.lock().unwrap();
            fills.push(fill.clone());
            Ok(())
        }

        async fn save_batch(&self, fills_to_add: &[Fill]) -> Result<(), RepositoryError> {
            let mut fills = self.fills.lock().unwrap();
            fills.extend_from_slice(fills_to_add);
            Ok(())
        }

        async fn get_by_order(&self, order_id: OrderId) -> Result<Vec<Fill>, RepositoryError> {
            let fills = self.fills.lock().unwrap();
            Ok(fills
                .iter()
                .filter(|f| f.order_id() == order_id)
                .cloned()
                .collect())
        }

        async fn get_by_symbol_range(
            &self,
            symbol: &Symbol,
            start: DateTime<Utc>,
            end: DateTime<Utc>,
        ) -> Result<Vec<Fill>, RepositoryError> {
            let fills = self.fills.lock().unwrap();
            Ok(fills
                .iter()
                .filter(|f| {
                    f.symbol() == symbol && f.timestamp() >= start && f.timestamp() <= end
                })
                .cloned()
                .collect())
        }

        async fn get_recent(&self, n: usize) -> Result<Vec<Fill>, RepositoryError> {
            let fills = self.fills.lock().unwrap();
            let mut result: Vec<Fill> = fills.iter().rev().take(n).cloned().collect();
            result.reverse(); // Maintain chronological order
            Ok(result)
        }
    }

    // =============================================================================
    // MOCK POSITION REPOSITORY
    // =============================================================================

    /// Mock implementation of PositionRepository for testing
    #[derive(Debug, Clone)]
    struct MockPositionRepository {
        positions: Arc<Mutex<HashMap<Symbol, Position>>>,
    }

    impl MockPositionRepository {
        fn new() -> Self {
            Self {
                positions: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl PositionRepository for MockPositionRepository {
        async fn find_by_id(&self, _id: EntityId) -> DomainResult<Option<Position>> {
            Ok(None)
        }

        async fn find_by_account(&self, _account_id: EntityId) -> DomainResult<Vec<Position>> {
            Ok(Vec::new())
        }

        async fn find_by_account_and_symbol(
            &self,
            _account_id: EntityId,
            symbol: &Symbol,
        ) -> DomainResult<Option<Position>> {
            let positions = self.positions.lock().unwrap();
            Ok(positions.get(symbol).cloned())
        }

        async fn find_open_positions(&self) -> DomainResult<Vec<Position>> {
            let positions = self.positions.lock().unwrap();
            Ok(positions.values().cloned().collect())
        }

        async fn save(&self, _position: &Position) -> DomainResult<()> {
            Ok(())
        }

        async fn delete(&self, _id: EntityId) -> DomainResult<()> {
            Ok(())
        }

        async fn upsert(&self, position: &Position) -> Result<(), RepositoryError> {
            let mut positions = self.positions.lock().unwrap();
            positions.insert(position.symbol().clone(), position.clone());
            Ok(())
        }

        async fn get_by_symbol(&self, symbol: &Symbol) -> Result<Option<Position>, RepositoryError> {
            let positions = self.positions.lock().unwrap();
            Ok(positions.get(symbol).cloned())
        }

        async fn get_all_open(&self) -> Result<Vec<Position>, RepositoryError> {
            let positions = self.positions.lock().unwrap();
            Ok(positions.values().cloned().collect())
        }

        async fn get_by_side(&self, side: Side) -> Result<Vec<Position>, RepositoryError> {
            let positions = self.positions.lock().unwrap();
            Ok(positions
                .values()
                .filter(|p| p.side() == side)
                .cloned()
                .collect())
        }

        async fn close(&self, symbol: &Symbol) -> Result<(), RepositoryError> {
            let mut positions = self.positions.lock().unwrap();
            positions
                .remove(symbol)
                .ok_or_else(|| RepositoryError::NotFound {
                    entity: "Position".to_string(),
                    id: symbol.to_string(),
                })?;
            Ok(())
        }

        async fn exists(&self, symbol: &Symbol) -> Result<bool, RepositoryError> {
            let positions = self.positions.lock().unwrap();
            Ok(positions.contains_key(symbol))
        }

        async fn total_pnl(&self) -> Result<Money, RepositoryError> {
            let positions = self.positions.lock().unwrap();
            let mut total = Money::new(Decimal::ZERO, Currency::USD)
                .map_err(|e| RepositoryError::QueryFailed {
                    query: "total_pnl".to_string(),
                    reason: e.to_string(),
                })?;
            for pos in positions.values() {
                let pnl = pos.unrealized_pnl();
                total = total.add(&pnl).map_err(|e| RepositoryError::QueryFailed {
                    query: "total_pnl".to_string(),
                    reason: e.to_string(),
                })?;
            }
            Ok(total)
        }

        async fn total_exposure(&self) -> Result<Money, RepositoryError> {
            let positions = self.positions.lock().unwrap();
            let mut total = Money::new(Decimal::ZERO, Currency::USD)
                .map_err(|e| RepositoryError::QueryFailed {
                    query: "total_exposure".to_string(),
                    reason: e.to_string(),
                })?;
            for pos in positions.values() {
                // Calculate notional value: quantity * avg_entry_price
                let amount = pos.avg_entry_price().inner() * pos.quantity().inner();
                let exposure = Money::new(amount, pos.unrealized_pnl().currency())
                    .map_err(|e| RepositoryError::QueryFailed {
                        query: "total_exposure".to_string(),
                        reason: e.to_string(),
                    })?;
                total = total.add(&exposure).map_err(|e| RepositoryError::QueryFailed {
                    query: "total_exposure".to_string(),
                    reason: e.to_string(),
                })?;
            }
            Ok(total)
        }
    }

    // =============================================================================
    // TESTS - ORDER REPOSITORY
    // =============================================================================

    #[tokio::test]
    async fn mock_order_repo_save_and_get() {
        let repo = MockOrderRepository::new();
        let order_id = OrderId::generate();
        
        // Initially should be None
        let result = repo.get(order_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn mock_order_repo_get_open() {
        let repo = MockOrderRepository::new();
        let open_orders = repo.get_open().await.unwrap();
        assert!(open_orders.is_empty());
    }

    #[tokio::test]
    async fn mock_order_repo_get_by_symbol() {
        let repo = MockOrderRepository::new();
        let symbol = Symbol::new("AAPL").unwrap();
        let orders = repo.get_by_symbol(&symbol).await.unwrap();
        assert!(orders.is_empty());
    }

    #[tokio::test]
    async fn mock_order_repo_count_by_status() {
        let repo = MockOrderRepository::new();
        let count = repo.count_by_status(OrderStatus::Created).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn mock_order_repo_get_history() {
        let repo = MockOrderRepository::new();
        let end = Utc::now();
        let start = end - Duration::days(7);
        let orders = repo.get_history(start, end).await.unwrap();
        assert!(orders.is_empty());
    }

    #[tokio::test]
    async fn mock_order_repo_update() {
        let repo = MockOrderRepository::new();
        // Mock update returns Ok for any order
        assert!(repo.update(&create_test_order()).await.is_ok());
    }

    // =============================================================================
    // TESTS - FILL REPOSITORY
    // =============================================================================

    #[tokio::test]
    async fn mock_fill_repo_save() {
        let repo = MockFillRepository::new();
        let fill = create_test_fill();
        assert!(repo.save(&fill).await.is_ok());
    }

    #[tokio::test]
    async fn mock_fill_repo_save_batch() {
        let repo = MockFillRepository::new();
        let fills = vec![create_test_fill(), create_test_fill()];
        assert!(repo.save_batch(&fills).await.is_ok());
    }

    #[tokio::test]
    async fn mock_fill_repo_get_by_order() {
        let repo = MockFillRepository::new();
        let order_id = OrderId::generate();
        let fills = repo.get_by_order(order_id).await.unwrap();
        assert!(fills.is_empty());
    }

    #[tokio::test]
    async fn mock_fill_repo_get_by_symbol_range() {
        let repo = MockFillRepository::new();
        let symbol = Symbol::new("AAPL").unwrap();
        let end = Utc::now();
        let start = end - Duration::days(1);
        let fills = repo.get_by_symbol_range(&symbol, start, end).await.unwrap();
        assert!(fills.is_empty());
    }

    #[tokio::test]
    async fn mock_fill_repo_get_recent() {
        let repo = MockFillRepository::new();
        let fills = repo.get_recent(10).await.unwrap();
        assert!(fills.is_empty());
    }

    // =============================================================================
    // TESTS - POSITION REPOSITORY
    // =============================================================================

    #[tokio::test]
    async fn mock_position_repo_upsert() {
        let repo = MockPositionRepository::new();
        let position = create_test_position();
        assert!(repo.upsert(&position).await.is_ok());
        
        // Verify it exists
        assert!(repo.exists(position.symbol()).await.unwrap());
    }

    #[tokio::test]
    async fn mock_position_repo_get_by_symbol() {
        let repo = MockPositionRepository::new();
        let symbol = Symbol::new("AAPL").unwrap();
        let position = repo.get_by_symbol(&symbol).await.unwrap();
        assert!(position.is_none());
    }

    #[tokio::test]
    async fn mock_position_repo_close() {
        let repo = MockPositionRepository::new();
        let position = create_test_position();
        let symbol = position.symbol().clone();
        
        // First upsert the position
        repo.upsert(&position).await.unwrap();
        assert!(repo.exists(&symbol).await.unwrap());
        
        // Then close it
        assert!(repo.close(&symbol).await.is_ok());
        assert!(!repo.exists(&symbol).await.unwrap());
        
        // Closing non-existent should fail
        assert!(repo.close(&symbol).await.is_err());
    }

    #[tokio::test]
    async fn mock_position_repo_get_by_side() {
        let repo = MockPositionRepository::new();
        let positions = repo.get_by_side(Side::Buy).await.unwrap();
        assert!(positions.is_empty());
    }

    #[tokio::test]
    async fn mock_position_repo_total_pnl() {
        let repo = MockPositionRepository::new();
        let pnl = repo.total_pnl().await.unwrap();
        assert_eq!(pnl.amount(), Decimal::ZERO);
        assert_eq!(pnl.currency(), Currency::USD);
    }

    #[tokio::test]
    async fn mock_position_repo_total_exposure() {
        let repo = MockPositionRepository::new();
        let exposure = repo.total_exposure().await.unwrap();
        assert_eq!(exposure.amount(), Decimal::ZERO);
        assert_eq!(exposure.currency(), Currency::USD);
    }

    // =============================================================================
    // TEST HELPERS
    // =============================================================================

    use crate::entities::{Order, OrderStatus, OrderType, Position, PositionDirection};
    use crate::values::{Currency, OrderId, Price, Quantity, Side, Symbol};

    fn create_test_order() -> Order {
        Order::new(
            Uuid::new_v4(), // account_id
            Symbol::new("AAPL").unwrap(),
            Side::Buy,
            OrderType::Market,
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            None, // limit_price
            None, // stop_price
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
            Uuid::new_v4(), // account_id
            Symbol::new("AAPL").unwrap(),
            PositionDirection::Long,
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            Price::new(Decimal::new(15000, 2)).unwrap(),
            Currency::USD,
        )
    }
}
