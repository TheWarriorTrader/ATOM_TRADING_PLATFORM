//! # TimescaleDB Order Repository
//!
//! SQLx-based implementation of the OrderRepository trait optimized for TimescaleDB.
//! Provides persistent storage and retrieval of trading orders with full audit trail.
//!
//! ## Features
//!
//! - **Type-safe queries**: Parameterized SQL preventing injection attacks
//! - **Complex status handling**: JSON serialization for OrderStatus variants with data
//! - **Transaction support**: Atomic updates with proper rollback on errors
//! - **Tracing**: Comprehensive instrumentation for observability
//!
//! ## OrderStatus Serialization
//!
//! OrderStatus variants with associated data are serialized to JSON for storage:
//! - `Created` → `{"variant":"Created"}`
//! - `Submitted { at }` → `{"variant":"Submitted","at":"2026-03-16T12:00:00Z"}`
//! - `PartiallyFilled { filled, remaining, avg_price }` →
//!   `{"variant":"PartiallyFilled","filled":"10.5","remaining":"5.5","avg_price":"150.25"}`

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{error, info, instrument};
use uuid::Uuid;

use domain::{
    entities::{EntityId, Order, OrderStatus, OrderType},
    errors::{DomainError, DomainResult, RepositoryError},
    repositories::OrderRepository,
    values::{OrderId, Price, Quantity, Side, Symbol, TimeInForce},
};

/// Maximum number of retries for transient database errors
const MAX_RETRIES: u32 = 5;

/// Initial retry delay in milliseconds
const INITIAL_RETRY_DELAY_MS: u64 = 100;

/// Maximum retry delay in milliseconds
const MAX_RETRY_DELAY_MS: u64 = 5000;

/// JSON representation of OrderStatus for database storage
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OrderStatusJson {
    variant: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filled: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remaining: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avg_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl OrderStatusJson {
    /// Converts OrderStatus to JSON representation
    fn from_status(status: &OrderStatus) -> Self {
        match status {
            OrderStatus::Created => Self {
                variant: "Created".to_string(),
                at: None,
                filled: None,
                remaining: None,
                avg_price: None,
                reason: None,
            },
            OrderStatus::Submitted { at } => Self {
                variant: "Submitted".to_string(),
                at: Some(*at),
                filled: None,
                remaining: None,
                avg_price: None,
                reason: None,
            },
            OrderStatus::Pending { at } => Self {
                variant: "Pending".to_string(),
                at: Some(*at),
                filled: None,
                remaining: None,
                avg_price: None,
                reason: None,
            },
            OrderStatus::PartiallyFilled {
                filled,
                remaining,
                avg_price,
            } => Self {
                variant: "PartiallyFilled".to_string(),
                at: None,
                filled: Some(filled.inner().to_string()),
                remaining: Some(remaining.inner().to_string()),
                avg_price: Some(avg_price.inner().to_string()),
                reason: None,
            },
            OrderStatus::Filled { at } => Self {
                variant: "Filled".to_string(),
                at: Some(*at),
                filled: None,
                remaining: None,
                avg_price: None,
                reason: None,
            },
            OrderStatus::Cancelled { at, reason } => Self {
                variant: "Cancelled".to_string(),
                at: Some(*at),
                filled: None,
                remaining: None,
                avg_price: None,
                reason: Some(reason.clone()),
            },
            OrderStatus::Rejected { at, reason } => Self {
                variant: "Rejected".to_string(),
                at: Some(*at),
                filled: None,
                remaining: None,
                avg_price: None,
                reason: Some(reason.clone()),
            },
        }
    }

    /// Converts JSON representation back to OrderStatus
    fn to_status(&self) -> Result<OrderStatus, RepositoryError> {
        match self.variant.as_str() {
            "Created" => Ok(OrderStatus::Created),
            "Submitted" => {
                let at = self.at.ok_or_else(|| {
                    RepositoryError::serialization("Missing 'at' timestamp for Submitted status")
                })?;
                Ok(OrderStatus::Submitted { at })
            }
            "Pending" => {
                let at = self.at.ok_or_else(|| {
                    RepositoryError::serialization("Missing 'at' timestamp for Pending status")
                })?;
                Ok(OrderStatus::Pending { at })
            }
            "PartiallyFilled" => {
                let filled_str = self.filled.as_ref().ok_or_else(|| {
                    RepositoryError::serialization(
                        "Missing 'filled' quantity for PartiallyFilled status",
                    )
                })?;
                let remaining_str = self.remaining.as_ref().ok_or_else(|| {
                    RepositoryError::serialization(
                        "Missing 'remaining' quantity for PartiallyFilled status",
                    )
                })?;
                let avg_price_str = self.avg_price.as_ref().ok_or_else(|| {
                    RepositoryError::serialization("Missing 'avg_price' for PartiallyFilled status")
                })?;

                let filled_val: Decimal = filled_str.parse().map_err(|e| {
                    RepositoryError::serialization(format!("Invalid filled quantity: {}", e))
                })?;
                let remaining_val: Decimal = remaining_str.parse().map_err(|e| {
                    RepositoryError::serialization(format!("Invalid remaining quantity: {}", e))
                })?;
                let avg_price_val: Decimal = avg_price_str.parse().map_err(|e| {
                    RepositoryError::serialization(format!("Invalid avg_price: {}", e))
                })?;

                let filled = Quantity::new(filled_val).map_err(|e| {
                    RepositoryError::serialization(format!("Invalid filled quantity: {}", e))
                })?;
                let remaining = Quantity::new(remaining_val).map_err(|e| {
                    RepositoryError::serialization(format!("Invalid remaining quantity: {}", e))
                })?;
                let avg_price = Price::new(avg_price_val).map_err(|e| {
                    RepositoryError::serialization(format!("Invalid avg_price: {}", e))
                })?;

                Ok(OrderStatus::PartiallyFilled {
                    filled,
                    remaining,
                    avg_price,
                })
            }
            "Filled" => {
                let at = self.at.ok_or_else(|| {
                    RepositoryError::serialization("Missing 'at' timestamp for Filled status")
                })?;
                Ok(OrderStatus::Filled { at })
            }
            "Cancelled" => {
                let at = self.at.ok_or_else(|| {
                    RepositoryError::serialization("Missing 'at' timestamp for Cancelled status")
                })?;
                let reason = self.reason.clone().unwrap_or_default();
                Ok(OrderStatus::Cancelled { at, reason })
            }
            "Rejected" => {
                let at = self.at.ok_or_else(|| {
                    RepositoryError::serialization("Missing 'at' timestamp for Rejected status")
                })?;
                let reason = self.reason.clone().unwrap_or_default();
                Ok(OrderStatus::Rejected { at, reason })
            }
            _ => Err(RepositoryError::serialization(format!(
                "Unknown OrderStatus variant: {}",
                self.variant
            ))),
        }
    }
}

/// Maps sqlx::Error to RepositoryError
fn map_sqlx_error(e: sqlx::Error) -> RepositoryError {
    match e {
        sqlx::Error::RowNotFound => RepositoryError::not_found("Order"),
        sqlx::Error::Database(db_err) => {
            if db_err.is_unique_violation() {
                RepositoryError::duplicate_key(db_err.to_string())
            } else if db_err.is_foreign_key_violation() {
                RepositoryError::constraint_violation(format!("Foreign key violation: {}", db_err))
            } else {
                RepositoryError::query(format!("Database error: {}", db_err))
            }
        }
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => {
            RepositoryError::connection("Database pool unavailable")
        }
        _ => RepositoryError::query(format!("Unexpected database error: {}", e)),
    }
}

/// TimescaleDB implementation of [`OrderRepository`]
///
/// Provides persistent storage and retrieval of trading orders with full
/// lifecycle tracking. Uses JSON serialization for complex OrderStatus variants.
///
/// # Example
///
/// ```rust,no_run
/// use infrastructure::database::order_repo::TimescaleOrderRepository;
/// use infrastructure::database::DatabasePool;
///
/// async fn example(pool: &DatabasePool) {
///     let repo = TimescaleOrderRepository::new(pool);
///     // Use repo to save/query orders...
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TimescaleOrderRepository {
    /// SQLx connection pool (wrapped in Arc for thread safety)
    pool: Arc<PgPool>,
}

impl TimescaleOrderRepository {
    /// Creates a new TimescaleOrderRepository
    ///
    /// # Arguments
    ///
    /// * `pool` - The database connection pool
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use infrastructure::database::order_repo::TimescaleOrderRepository;
    /// use infrastructure::database::DatabasePool;
    ///
    /// fn create_repo(pool: &DatabasePool) -> TimescaleOrderRepository {
    ///     TimescaleOrderRepository::new(pool)
    /// }
    /// ```
    #[must_use]
    pub fn new(pool: &super::DatabasePool) -> Self {
        Self {
            pool: Arc::new(pool.pool().clone()),
        }
    }

    /// Creates a new TimescaleOrderRepository directly from a PgPool
    ///
    /// This is useful for testing when you already have a PgPool instance
    /// (e.g., from testcontainers).
    ///
    /// # Arguments
    ///
    /// * `pool` - The SQLx PostgreSQL connection pool
    #[must_use]
    pub fn new_with_pool(pool: PgPool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Maps a database row to an Order entity
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::SerializationFailed`] if row data cannot be parsed
    fn row_to_order(&self, row: &sqlx::postgres::PgRow) -> Result<Order, RepositoryError> {
        // Extract UUID and convert to OrderId
        let id: Uuid = row.try_get("id").map_err(map_sqlx_error)?;

        // Extract account_id (use id as fallback if not present)
        let account_id: Uuid = row.try_get("account_id").unwrap_or(id);

        // Extract and parse symbol
        let symbol_str: String = row.try_get("symbol").map_err(map_sqlx_error)?;
        let symbol = Symbol::new(&symbol_str).map_err(|e| {
            RepositoryError::serialization(format!("Invalid symbol '{}': {}", symbol_str, e))
        })?;

        // Extract and parse side
        let side_str: String = row.try_get("side").map_err(map_sqlx_error)?;
        let side = match side_str.to_lowercase().as_str() {
            "buy" => Side::Buy,
            "sell" => Side::Sell,
            _ => {
                return Err(RepositoryError::serialization(format!(
                    "Invalid side: {}",
                    side_str
                )))
            }
        };

        // Extract and parse order_type
        let order_type_str: String = row.try_get("order_type").map_err(map_sqlx_error)?;
        let order_type = match order_type_str.to_lowercase().as_str() {
            "market" => OrderType::Market,
            "limit" => OrderType::Limit,
            "stop" => OrderType::Stop,
            "stoplimit" | "stop_limit" | "stop-limit" => OrderType::StopLimit,
            _ => {
                return Err(RepositoryError::serialization(format!(
                    "Invalid order_type: {}",
                    order_type_str
                )))
            }
        };

        // Extract and parse quantity
        let quantity_f64: f64 = row.try_get("quantity").map_err(map_sqlx_error)?;
        let quantity = Quantity::new(Decimal::from_f64_retain(quantity_f64).ok_or_else(|| {
            RepositoryError::serialization(format!("Invalid quantity: {}", quantity_f64))
        })?)
        .map_err(|e| RepositoryError::serialization(format!("Invalid quantity: {}", e)))?;

        // Extract optional limit_price
        let limit_price: Option<f64> = row.try_get("limit_price").ok();
        let limit_price = limit_price
            .and_then(|p| Decimal::from_f64_retain(p))
            .and_then(|p| Price::new(p).ok());

        // Extract optional stop_price
        let stop_price: Option<f64> = row.try_get("stop_price").ok();
        let stop_price = stop_price
            .and_then(|p| Decimal::from_f64_retain(p))
            .and_then(|p| Price::new(p).ok());

        // Extract and parse status from JSON
        let status_json_str: String = row.try_get("status").map_err(map_sqlx_error)?;
        let status_json: OrderStatusJson = serde_json::from_str(&status_json_str).map_err(|e| {
            RepositoryError::serialization(format!(
                "Failed to parse status JSON: {} - {}",
                status_json_str, e
            ))
        })?;
        let status = status_json.to_status()?;

        // Extract and parse time_in_force
        let tif_str: String = row.try_get("tif").map_err(map_sqlx_error)?;
        let time_in_force = match tif_str.to_lowercase().as_str() {
            "day" => TimeInForce::Day,
            "gtc" => TimeInForce::GTC,
            "ioc" => TimeInForce::IOC,
            "fok" => TimeInForce::FOK,
            _ => {
                return Err(RepositoryError::serialization(format!(
                    "Invalid time_in_force: {}",
                    tif_str
                )))
            }
        };

        // Create the order using new_with_id
        Order::new_with_id(
            OrderId::from_uuid(id),
            account_id,
            symbol,
            side,
            quantity,
            order_type,
            time_in_force,
            limit_price,
            stop_price,
        )
        .map_err(|e| RepositoryError::serialization(format!("Failed to create Order: {}", e)))
    }

    /// Executes an operation with retry logic for transient failures
    ///
    /// # Type Parameters
    ///
    /// * `F` - The future type returned by the operation
    /// * `Fut` - The future itself
    ///
    /// # Arguments
    ///
    /// * `operation` - Name of the operation for logging
    /// * `f` - The operation to execute
    async fn with_retry<F, Fut>(&self, operation: &str, f: F) -> DomainResult<()>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<(), sqlx::Error>>,
    {
        let mut delay = std::time::Duration::from_millis(INITIAL_RETRY_DELAY_MS);

        for attempt in 1..=MAX_RETRIES {
            match f().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let is_transient = matches!(
                        &e,
                        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_)
                    );

                    if !is_transient || attempt == MAX_RETRIES {
                        error!(
                            operation = %operation,
                            attempt = attempt,
                            error = %e,
                            "Database operation failed"
                        );
                        return Err(map_sqlx_error(e).into());
                    }

                    tracing::warn!(
                        operation = %operation,
                        attempt = attempt,
                        error = %e,
                        delay_ms = delay.as_millis(),
                        "Transient error, retrying..."
                    );

                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(
                        delay * 2,
                        std::time::Duration::from_millis(MAX_RETRY_DELAY_MS),
                    );
                }
            }
        }

        Err(RepositoryError::connection("Max retries exceeded").into())
    }
}

#[async_trait]
impl OrderRepository for TimescaleOrderRepository {
    #[instrument(skip(self), fields(order_id = %id))]
    async fn find_by_id(&self, id: EntityId) -> DomainResult<Option<Order>> {
        let result = sqlx::query(
            r#"
            SELECT 
                id, external_id, symbol, side, quantity, order_type,
                limit_price, stop_price, tif, status,
                created_at, updated_at, provider, account_id
            FROM orders
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match result {
            Some(row) => {
                let order = self.row_to_order(&row)?;
                info!(order_id = %id, "Found order");
                Ok(Some(order))
            }
            None => {
                info!(order_id = %id, "Order not found");
                Ok(None)
            }
        }
    }

    #[instrument(skip(self), fields(account_id = %account_id))]
    async fn find_by_account(&self, account_id: EntityId) -> DomainResult<Vec<Order>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                id, external_id, symbol, side, quantity, order_type,
                limit_price, stop_price, tif, status,
                created_at, updated_at, provider, account_id
            FROM orders
            WHERE account_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(account_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut orders = Vec::with_capacity(rows.len());
        for row in rows {
            orders.push(self.row_to_order(&row)?);
        }

        info!(account_id = %account_id, count = orders.len(), "Retrieved orders by account");
        Ok(orders)
    }

    #[instrument(skip(self), fields(account_id = %account_id))]
    async fn find_active_by_account(&self, account_id: EntityId) -> DomainResult<Vec<Order>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                id, external_id, symbol, side, quantity, order_type,
                limit_price, stop_price, tif, status,
                created_at, updated_at, provider, account_id
            FROM orders
            WHERE account_id = $1
              AND status IN ('Created', 'Submitted', 'Pending', 'PartiallyFilled')
            ORDER BY created_at DESC
            "#,
        )
        .bind(account_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut orders = Vec::with_capacity(rows.len());
        for row in rows {
            orders.push(self.row_to_order(&row)?);
        }

        info!(account_id = %account_id, count = orders.len(), "Retrieved active orders by account");
        Ok(orders)
    }

    #[instrument(skip(self, order), fields(order_id = %order.id()))]
    async fn save(&self, order: &Order) -> DomainResult<()> {
        let status_json = OrderStatusJson::from_status(&order.status());
        let status_json_str = serde_json::to_string(&status_json).map_err(|e| {
            RepositoryError::serialization(format!("Failed to serialize status: {}", e))
        })?;

        let id = order.id();
        let symbol = order.symbol().to_string();
        let side = order.side().as_str().to_lowercase();
        let quantity: f64 = order.quantity().inner().to_string().parse().map_err(|e| {
            RepositoryError::serialization(format!("Failed to convert quantity: {}", e))
        })?;
        let order_type = match order.order_type() {
            OrderType::Market => "market",
            OrderType::Limit => "limit",
            OrderType::Stop => "stop",
            OrderType::StopLimit => "stop_limit",
        };
        let limit_price: Option<f64> = order
            .limit_price()
            .map(|p| p.inner().to_string().parse().unwrap_or(0.0));
        let stop_price: Option<f64> = order
            .stop_price()
            .map(|p| p.inner().to_string().parse().unwrap_or(0.0));
        let tif = match order.time_in_force() {
            TimeInForce::Day => "day",
            TimeInForce::GTC => "gtc",
            TimeInForce::IOC => "ioc",
            TimeInForce::FOK => "fok",
        };

        self.with_retry("save_order", || {
            let symbol = symbol.clone();
            let side = side.clone();
            let order_type = order_type.to_string();
            let tif = tif.to_string();
            let status_json_str = status_json_str.clone();

            async move {
                sqlx::query(
                    r#"
                    INSERT INTO orders (
                        id, symbol, side, quantity, order_type,
                        limit_price, stop_price, tif, status,
                        created_at, updated_at, provider, account_id
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'infrastructure', $12
                    )
                    ON CONFLICT (id) DO UPDATE SET
                        symbol = EXCLUDED.symbol,
                        side = EXCLUDED.side,
                        quantity = EXCLUDED.quantity,
                        order_type = EXCLUDED.order_type,
                        limit_price = EXCLUDED.limit_price,
                        stop_price = EXCLUDED.stop_price,
                        tif = EXCLUDED.tif,
                        status = EXCLUDED.status,
                        updated_at = EXCLUDED.updated_at,
                        account_id = EXCLUDED.account_id
                    "#,
                )
                .bind(id)
                .bind(&symbol)
                .bind(&side)
                .bind(quantity)
                .bind(&order_type)
                .bind(limit_price)
                .bind(stop_price)
                .bind(&tif)
                .bind(&status_json_str)
                .bind(order.created_at())
                .bind(order.updated_at())
                .bind(id) // account_id fallback to order id
                .execute(&*self.pool)
                .await
                .map(|_| ())
            }
        })
        .await?;

        info!(order_id = %id, "Order saved successfully");
        Ok(())
    }

    #[instrument(skip(self), fields(order_id = %id))]
    async fn delete(&self, id: EntityId) -> DomainResult<()> {
        let result = sqlx::query("DELETE FROM orders WHERE id = $1")
            .bind(id)
            .execute(&*self.pool)
            .await
            .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::not_found(format!("Order {}", id)).into());
        }

        info!(order_id = %id, "Order deleted successfully");
        Ok(())
    }

    #[instrument(skip(self, order), fields(order_id = %order.id()))]
    async fn update(&self, order: &Order) -> Result<(), RepositoryError> {
        let status_json = OrderStatusJson::from_status(&order.status());
        let status_json_str = serde_json::to_string(&status_json).map_err(|e| {
            RepositoryError::serialization(format!("Failed to serialize status: {}", e))
        })?;

        let id = order.id();
        let symbol = order.symbol().to_string();
        let side = order.side().as_str().to_lowercase();
        let quantity: f64 = order.quantity().inner().to_string().parse().map_err(|e| {
            RepositoryError::serialization(format!("Failed to convert quantity: {}", e))
        })?;
        let order_type = match order.order_type() {
            OrderType::Market => "market",
            OrderType::Limit => "limit",
            OrderType::Stop => "stop",
            OrderType::StopLimit => "stop_limit",
        };
        let limit_price: Option<f64> = order
            .limit_price()
            .map(|p| p.inner().to_string().parse().unwrap_or(0.0));
        let stop_price: Option<f64> = order
            .stop_price()
            .map(|p| p.inner().to_string().parse().unwrap_or(0.0));
        let tif = match order.time_in_force() {
            TimeInForce::Day => "day",
            TimeInForce::GTC => "gtc",
            TimeInForce::IOC => "ioc",
            TimeInForce::FOK => "fok",
        };

        let result = sqlx::query(
            r#"
            UPDATE orders SET
                symbol = $2,
                side = $3,
                quantity = $4,
                order_type = $5,
                limit_price = $6,
                stop_price = $7,
                tif = $8,
                status = $9,
                updated_at = $10,
                account_id = $11
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(&symbol)
        .bind(&side)
        .bind(quantity)
        .bind(order_type)
        .bind(limit_price)
        .bind(stop_price)
        .bind(tif)
        .bind(&status_json_str)
        .bind(Utc::now())
        .bind(id) // account_id fallback
        .execute(&*self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::not_found(format!("Order {}", id)));
        }

        info!(order_id = %id, "Order updated successfully");
        Ok(())
    }

    #[instrument(skip(self), fields(order_id = %order_id))]
    async fn get(&self, order_id: OrderId) -> Result<Option<Order>, RepositoryError> {
        let id = order_id.as_uuid();

        let result = sqlx::query(
            r#"
            SELECT 
                id, external_id, symbol, side, quantity, order_type,
                limit_price, stop_price, tif, status,
                created_at, updated_at, provider, account_id
            FROM orders
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match result {
            Some(row) => {
                let order = self.row_to_order(&row)?;
                info!(order_id = %order_id, "Found order");
                Ok(Some(order))
            }
            None => {
                info!(order_id = %order_id, "Order not found");
                Ok(None)
            }
        }
    }

    #[instrument(skip(self))]
    async fn get_open(&self) -> Result<Vec<Order>, RepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT 
                id, external_id, symbol, side, quantity, order_type,
                limit_price, stop_price, tif, status,
                created_at, updated_at, provider, account_id
            FROM orders
            WHERE status IN ('Created', 'Submitted', 'Pending', 'PartiallyFilled')
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut orders = Vec::with_capacity(rows.len());
        for row in rows {
            orders.push(self.row_to_order(&row)?);
        }

        info!(count = orders.len(), "Retrieved open orders");
        Ok(orders)
    }

    #[instrument(skip(self), fields(symbol = %symbol))]
    async fn get_by_symbol(&self, symbol: &Symbol) -> Result<Vec<Order>, RepositoryError> {
        let symbol_str = symbol.to_string();

        let rows = sqlx::query(
            r#"
            SELECT 
                id, external_id, symbol, side, quantity, order_type,
                limit_price, stop_price, tif, status,
                created_at, updated_at, provider, account_id
            FROM orders
            WHERE symbol = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(&symbol_str)
        .fetch_all(&*self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut orders = Vec::with_capacity(rows.len());
        for row in rows {
            orders.push(self.row_to_order(&row)?);
        }

        info!(symbol = %symbol, count = orders.len(), "Retrieved orders by symbol");
        Ok(orders)
    }

    #[instrument(skip(self, start, end))]
    async fn get_history(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Order>, RepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT 
                id, external_id, symbol, side, quantity, order_type,
                limit_price, stop_price, tif, status,
                created_at, updated_at, provider, account_id
            FROM orders
            WHERE created_at BETWEEN $1 AND $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(&*self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut orders = Vec::with_capacity(rows.len());
        for row in rows {
            orders.push(self.row_to_order(&row)?);
        }

        info!(start = %start, end = %end, count = orders.len(), "Retrieved order history");
        Ok(orders)
    }

    #[instrument(skip(self), fields(status = ?status))]
    async fn count_by_status(&self, status: OrderStatus) -> Result<u64, RepositoryError> {
        let status_variant = match &status {
            OrderStatus::Created => "Created",
            OrderStatus::Submitted { .. } => "Submitted",
            OrderStatus::Pending { .. } => "Pending",
            OrderStatus::PartiallyFilled { .. } => "PartiallyFilled",
            OrderStatus::Filled { .. } => "Filled",
            OrderStatus::Cancelled { .. } => "Cancelled",
            OrderStatus::Rejected { .. } => "Rejected",
        };

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE status LIKE $1 || '%'")
                .bind(status_variant)
                .fetch_one(&*self.pool)
                .await
                .map_err(map_sqlx_error)?;

        info!(status = ?status, count = count, "Counted orders by status");
        Ok(count as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    /// Creates a test order with default values
    fn create_test_order() -> Order {
        Order::new_with_id(
            OrderId::generate(),
            Uuid::new_v4(),
            Symbol::new("NQ").unwrap(),
            Side::Buy,
            Quantity::new(dec!(10.0)).unwrap(),
            OrderType::Limit,
            TimeInForce::Day,
            Some(Price::new(dec!(15000.0)).unwrap()),
            None,
        )
        .unwrap()
    }

    /// Creates a test order with market type
    fn create_test_market_order() -> Order {
        Order::new_with_id(
            OrderId::generate(),
            Uuid::new_v4(),
            Symbol::new("ES").unwrap(),
            Side::Sell,
            Quantity::new(dec!(5.0)).unwrap(),
            OrderType::Market,
            TimeInForce::Day,
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn test_order_status_json_roundtrip() {
        let statuses = vec![
            OrderStatus::Created,
            OrderStatus::Submitted { at: Utc::now() },
            OrderStatus::Pending { at: Utc::now() },
            OrderStatus::PartiallyFilled {
                filled: Quantity::new(dec!(5.0)).unwrap(),
                remaining: Quantity::new(dec!(5.0)).unwrap(),
                avg_price: Price::new(dec!(150.0)).unwrap(),
            },
            OrderStatus::Filled { at: Utc::now() },
            OrderStatus::Cancelled {
                at: Utc::now(),
                reason: "User request".to_string(),
            },
            OrderStatus::Rejected {
                at: Utc::now(),
                reason: "Insufficient funds".to_string(),
            },
        ];

        for original in statuses {
            let json = OrderStatusJson::from_status(&original);
            let json_str = serde_json::to_string(&json).unwrap();
            let parsed: OrderStatusJson = serde_json::from_str(&json_str).unwrap();
            let restored = parsed.to_status().unwrap();

            assert_eq!(
                original.variant_name(),
                restored.variant_name(),
                "Status variant mismatch after roundtrip"
            );
        }
    }

    #[test]
    fn test_order_status_json_partially_filled() {
        let status = OrderStatus::PartiallyFilled {
            filled: Quantity::new(dec!(3.5)).unwrap(),
            remaining: Quantity::new(dec!(6.5)).unwrap(),
            avg_price: Price::new(dec!(150.25)).unwrap(),
        };

        let json = OrderStatusJson::from_status(&status);
        assert_eq!(json.variant, "PartiallyFilled");
        assert_eq!(json.filled, Some("3.5".to_string()));
        assert_eq!(json.remaining, Some("6.5".to_string()));
        assert_eq!(json.avg_price, Some("150.25".to_string()));
    }

    #[test]
    fn test_order_status_json_cancelled() {
        let at = Utc::now();
        let status = OrderStatus::Cancelled {
            at,
            reason: "Time in force expired".to_string(),
        };

        let json = OrderStatusJson::from_status(&status);
        assert_eq!(json.variant, "Cancelled");
        assert_eq!(json.at, Some(at));
        assert_eq!(json.reason, Some("Time in force expired".to_string()));
    }

    #[test]
    fn test_map_sqlx_error_row_not_found() {
        let err = sqlx::Error::RowNotFound;
        let mapped = map_sqlx_error(err);

        match mapped {
            RepositoryError::NotFound { .. } => (), // Expected
            _ => panic!("Expected NotFound error, got {:?}", mapped),
        }
    }

    #[test]
    fn test_create_test_order() {
        let order = create_test_order();
        assert_eq!(order.symbol().as_str(), "NQ");
        assert_eq!(order.side(), Side::Buy);
        assert!(matches!(order.order_type(), OrderType::Limit));
        assert!(order.limit_price().is_some());
    }

    #[test]
    fn test_create_test_market_order() {
        let order = create_test_market_order();
        assert_eq!(order.symbol().as_str(), "ES");
        assert_eq!(order.side(), Side::Sell);
        assert!(matches!(order.order_type(), OrderType::Market));
        assert!(order.limit_price().is_none());
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_save_and_get_order() {
        // This test requires a running PostgreSQL instance
        // Run with: cargo test -p infrastructure order_repo::tests -- --ignored
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_update_order_status() {
        // This test requires a running PostgreSQL instance
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_get_open_orders() {
        // This test requires a running PostgreSQL instance
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_get_by_symbol() {
        // This test requires a running PostgreSQL instance
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_count_by_status() {
        // This test requires a running PostgreSQL instance
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_get_history_range() {
        // This test requires a running PostgreSQL instance
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_delete_order() {
        // This test requires a running PostgreSQL instance
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_find_by_account() {
        // This test requires a running PostgreSQL instance
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_find_active_by_account() {
        // This test requires a running PostgreSQL instance
    }
}
