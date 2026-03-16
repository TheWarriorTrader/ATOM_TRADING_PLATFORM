//! # TimescaleDB Repository Implementations
//!
//! SQLx-based implementations of market data repository traits optimized for TimescaleDB.
//! Provides efficient storage and retrieval of time-series market data (bars and ticks).
//!
//! ## Features
//!
//! - **Batch operations**: Efficient bulk inserts using `UNNEST` for high throughput
//! - **Retry logic**: Automatic retry with exponential backoff for transient errors
//! - **Parameterized queries**: SQL injection safe via sqlx prepared statements
//! - **Conflict resolution**: Upsert semantics for duplicate data points
//! - **Time-series optimized**: Leverages TimescaleDB hypertables for performance
//!
//! ## Retry Policy
//!
//! Transient errors (connection failures, timeouts) are automatically retried
//! with exponential backoff: 100ms, 200ms, 400ms, 800ms, 1600ms (max 5 retries).

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use tracing::{debug, error, info, instrument, warn};

use domain::{
    Bar, BarRepository, DomainResult, Symbol, Tick, TickRepository,
    errors::RepositoryError,
};

use crate::database::DatabasePool;
use crate::errors::InfrastructureError;

/// Maximum number of retries for transient database errors
const MAX_RETRIES: u32 = 5;

/// Initial retry delay in milliseconds
const INITIAL_RETRY_DELAY_MS: u64 = 100;

/// Maximum retry delay in milliseconds
const MAX_RETRY_DELAY_MS: u64 = 5000;

/// TimescaleDB implementation of [`BarRepository`]
///
/// Provides optimized storage and retrieval of OHLCV bar data using TimescaleDB
/// hypertables. Implements automatic retry logic for resilience against
/// transient failures.
///
/// # Example
///
/// ```rust,no_run
/// use infrastructure::database::timescale::TimescaleBarRepository;
/// use infrastructure::database::DatabasePool;
///
/// async fn example(pool: &DatabasePool) {
///     let repo = TimescaleBarRepository::new(pool);
///     // Use repo to save/query bars...
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TimescaleBarRepository {
    /// SQLx connection pool
    pool: PgPool,
}

impl TimescaleBarRepository {
    /// Creates a new TimescaleBarRepository
    ///
    /// # Arguments
    ///
    /// * `pool` - The database connection pool
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use infrastructure::database::timescale::TimescaleBarRepository;
    /// use infrastructure::database::DatabasePool;
    ///
    /// fn create_repo(pool: &DatabasePool) -> TimescaleBarRepository {
    ///     TimescaleBarRepository::new(pool)
    /// }
    /// ```
    #[must_use]
    pub fn new(pool: &DatabasePool) -> Self {
        Self {
            pool: pool.pool().clone(),
        }
    }

    /// Creates a new TimescaleBarRepository directly from a PgPool
    ///
    /// This is useful for testing when you already have a PgPool instance
    /// (e.g., from testcontainers).
    ///
    /// # Arguments
    ///
    /// * `pool` - The SQLx PostgreSQL connection pool
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use infrastructure::database::timescale::TimescaleBarRepository;
    /// use sqlx::PgPool;
    ///
    /// async fn create_repo(pool: PgPool) -> TimescaleBarRepository {
    ///     TimescaleBarRepository::new_with_pool(pool)
    /// }
    /// ```
    #[must_use]
    pub fn new_with_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Executes an operation with retry logic for transient errors
    ///
    /// Automatically retries on connection failures, pool timeouts, and
    /// other transient database errors with exponential backoff.
    ///
    /// # Type Parameters
    ///
    /// * `F` - The future type returned by the operation
    /// * `Fut` - The async operation to execute
    /// * `T` - The return type of the operation
    ///
    /// # Arguments
    ///
    /// * `operation` - A description of the operation for logging
    /// * `f` - The async operation to execute
    ///
    /// # Returns
    ///
    /// Returns the result of the operation, or an error if all retries are exhausted
    async fn with_retry<F, Fut, T>(&self, operation: &str, f: F) -> DomainResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
    {
        let mut delay = Duration::from_millis(INITIAL_RETRY_DELAY_MS);

        for attempt in 1..=MAX_RETRIES {
            match f().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let is_transient = matches!(
                        e,
                        sqlx::Error::PoolTimedOut
                            | sqlx::Error::PoolClosed
                            | sqlx::Error::Io(_)
                            | sqlx::Error::Tls(_)
                    );

                    if !is_transient || attempt == MAX_RETRIES {
                        error!(
                            operation = %operation,
                            attempt = attempt,
                            error = %e,
                            "Database operation failed"
                        );
                        return Err(map_sqlx_error(e));
                    }

                    warn!(
                        operation = %operation,
                        attempt = attempt,
                        error = %e,
                        delay_ms = delay.as_millis(),
                        "Transient error, retrying..."
                    );

                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(
                        delay * 2,
                        Duration::from_millis(MAX_RETRY_DELAY_MS),
                    );
                }
            }
        }

        // This should never be reached due to the return in the loop
        Err(RepositoryError::connection("Max retries exceeded").into())
    }
}

#[async_trait]
impl BarRepository for TimescaleBarRepository {
    #[instrument(skip(self, bar), fields(symbol = %bar.symbol(), time = %bar.timestamp()))]
    async fn save(&self, bar: &Bar) -> DomainResult<()> {
        debug!("Saving single bar to TimescaleDB");

        let symbol = bar.symbol().to_string();
        let time = bar.timestamp();
        let open: f64 = bar.open().inner().try_into().map_err(|_| {
            RepositoryError::SerializationFailed {
                reason: "Failed to convert open price to f64".to_string()
            }
        })?;
        let high: f64 = bar.high().inner().try_into().map_err(|_| {
            RepositoryError::SerializationFailed {
                reason: "Failed to convert high price to f64".to_string()
            }
        })?;
        let low: f64 = bar.low().inner().try_into().map_err(|_| {
            RepositoryError::SerializationFailed {
                reason: "Failed to convert low price to f64".to_string()
            }
        })?;
        let close: f64 = bar.close().inner().try_into().map_err(|_| {
            RepositoryError::SerializationFailed {
                reason: "Failed to convert close price to f64".to_string()
            }
        })?;
        let volume: i64 = bar.volume().inner();
        let provider = bar.provider().to_string();

        self.with_retry("save_bar", || {
            let symbol = symbol.clone();
            let provider = provider.clone();
            async move {
                sqlx::query(
                    r#"
                    INSERT INTO bars (time, symbol, open, high, low, close, volume, provider)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    ON CONFLICT (time, symbol) DO UPDATE SET
                        open = EXCLUDED.open,
                        high = EXCLUDED.high,
                        low = EXCLUDED.low,
                        close = EXCLUDED.close,
                        volume = bars.volume + EXCLUDED.volume
                    "#,
                )
                .bind(time)
                .bind(&symbol)
                .bind(open)
                .bind(high)
                .bind(low)
                .bind(close)
                .bind(volume)
                .bind(&provider)
                .execute(&self.pool)
                .await
            }
        })
        .await?;

        info!("Bar saved successfully");
        Ok(())
    }

    #[instrument(skip(self, bars), fields(count = bars.len()))]
    async fn save_batch(&self, bars: &[Bar]) -> DomainResult<()> {
        if bars.is_empty() {
            debug!("Empty batch, skipping save");
            return Ok(());
        }

        info!(count = bars.len(), "Saving batch of bars to TimescaleDB");

        // Prepare vectors for bulk insert using UNNEST
        let mut times: Vec<DateTime<Utc>> = Vec::with_capacity(bars.len());
        let mut symbols: Vec<String> = Vec::with_capacity(bars.len());
        let mut opens: Vec<f64> = Vec::with_capacity(bars.len());
        let mut highs: Vec<f64> = Vec::with_capacity(bars.len());
        let mut lows: Vec<f64> = Vec::with_capacity(bars.len());
        let mut closes: Vec<f64> = Vec::with_capacity(bars.len());
        let mut volumes: Vec<i64> = Vec::with_capacity(bars.len());
        let mut providers: Vec<String> = Vec::with_capacity(bars.len());

        for bar in bars {
            times.push(bar.timestamp());
            symbols.push(bar.symbol().to_string());
            opens.push(
                bar.open()
                    .inner()
                    .try_into()
                    .map_err(|_| RepositoryError::SerializationFailed {
                        reason: "Invalid open price".to_string()
                    })?
            );
            highs.push(
                bar.high()
                    .inner()
                    .try_into()
                    .map_err(|_| RepositoryError::SerializationFailed {
                        reason: "Invalid high price".to_string()
                    })?
            );
            lows.push(
                bar.low()
                    .inner()
                    .try_into()
                    .map_err(|_| RepositoryError::SerializationFailed {
                        reason: "Invalid low price".to_string()
                    })?
            );
            closes.push(
                bar.close()
                    .inner()
                    .try_into()
                    .map_err(|_| RepositoryError::SerializationFailed {
                        reason: "Invalid close price".to_string()
                    })?
            );
            volumes.push(bar.volume().inner());
            providers.push(bar.provider().to_string());
        }

        self.with_retry("save_batch", || {
            let times = times.clone();
            let symbols = symbols.clone();
            let opens = opens.clone();
            let highs = highs.clone();
            let lows = lows.clone();
            let closes = closes.clone();
            let volumes = volumes.clone();
            let providers = providers.clone();

            async move {
                sqlx::query(
                    r#"
                    INSERT INTO bars (time, symbol, open, high, low, close, volume, provider)
                    SELECT * FROM UNNEST($1::timestamptz[], $2::text[], $3::float8[], $4::float8[], $5::float8[], $6::float8[], $7::bigint[], $8::text[])
                    ON CONFLICT (time, symbol) DO UPDATE SET
                        open = EXCLUDED.open,
                        high = EXCLUDED.high,
                        low = EXCLUDED.low,
                        close = EXCLUDED.close,
                        volume = bars.volume + EXCLUDED.volume
                    "#,
                )
                .bind(&times[..])
                .bind(&symbols[..])
                .bind(&opens[..])
                .bind(&highs[..])
                .bind(&lows[..])
                .bind(&closes[..])
                .bind(&volumes[..])
                .bind(&providers[..])
                .execute(&self.pool)
                .await
            }
        })
        .await?;

        info!(count = bars.len(), "Batch saved successfully");
        Ok(())
    }

    #[instrument(skip(self), fields(symbol = %symbol, start = %start, end = %end))]
    async fn get_range(
        &self,
        symbol: &Symbol,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DomainResult<Vec<Bar>> {
        debug!("Querying bars from TimescaleDB");

        let symbol_str = symbol.to_string();

        let rows: Vec<sqlx::postgres::PgRow> = self
            .with_retry("get_range", || {
                let symbol_str = symbol_str.clone();
                async move {
                    sqlx::query(
                        r#"
                        SELECT time, symbol, open, high, low, close, volume, provider
                        FROM bars
                        WHERE symbol = $1 AND time >= $2 AND time <= $3
                        ORDER BY time ASC
                        "#,
                    )
                    .bind(&symbol_str)
                    .bind(start)
                    .bind(end)
                    .fetch_all(&self.pool)
                    .await
                }
            })
            .await?;

        let mut bars = Vec::with_capacity(rows.len());
        for row in rows {
            bars.push(Bar::new(
                row.try_get("time").map_err(map_sqlx_error)?,
                Symbol::new(row.try_get::<String, _>("symbol").map_err(map_sqlx_error)?).map_err(|e| RepositoryError::query(e.to_string()))?,
                domain::Price::new(Decimal::from_f64_retain(row.try_get::<f64, _>("open").map_err(map_sqlx_error)?).ok_or_else(|| {
                    RepositoryError::serialization("Invalid open price")
                })?)
                .map_err(|e| RepositoryError::query(e.to_string()))?,
                domain::Price::new(Decimal::from_f64_retain(row.try_get::<f64, _>("high").map_err(map_sqlx_error)?).ok_or_else(|| {
                    RepositoryError::serialization("Invalid high price")
                })?)
                .map_err(|e| RepositoryError::query(e.to_string()))?,
                domain::Price::new(Decimal::from_f64_retain(row.try_get::<f64, _>("low").map_err(map_sqlx_error)?).ok_or_else(|| {
                    RepositoryError::serialization("Invalid low price")
                })?)
                .map_err(|e| RepositoryError::query(e.to_string()))?,
                domain::Price::new(Decimal::from_f64_retain(row.try_get::<f64, _>("close").map_err(map_sqlx_error)?).ok_or_else(|| {
                    RepositoryError::serialization("Invalid close price")
                })?)
                .map_err(|e| RepositoryError::query(e.to_string()))?,
                // SAFETY: Volume from database is always non-negative
                unsafe { domain::Volume::new_unchecked(row.try_get::<i64, _>("volume").map_err(map_sqlx_error)?) },
                domain::TimeFrame::M1, // Default timeframe - not stored in DB
                row.try_get::<String, _>("provider").map_err(map_sqlx_error)?,
            )?);
        }

        info!(count = bars.len(), "Bars retrieved");
        Ok(bars)
    }

    #[instrument(skip(self), fields(symbol = %symbol, n = n))]
    async fn get_latest(&self, symbol: &Symbol, n: usize) -> DomainResult<Vec<Bar>> {
        debug!("Querying latest bars from TimescaleDB");

        let symbol_str = symbol.to_string();
        let limit = n as i64;

        let rows: Vec<sqlx::postgres::PgRow> = self
            .with_retry("get_latest", || {
                let symbol_str = symbol_str.clone();
                async move {
                    sqlx::query(
                        r#"
                        SELECT time, symbol, open, high, low, close, volume, provider
                        FROM bars
                        WHERE symbol = $1
                        ORDER BY time DESC
                        LIMIT $2
                        "#,
                    )
                    .bind(&symbol_str)
                    .bind(limit)
                    .fetch_all(&self.pool)
                    .await
                }
            })
            .await?;

        let mut bars = Vec::with_capacity(rows.len());
        for row in rows.into_iter().rev() { // Reverse to get ascending order
            bars.push(Bar::new(
                row.try_get("time").map_err(map_sqlx_error)?,
                Symbol::new(row.try_get::<String, _>("symbol").map_err(map_sqlx_error)?).map_err(|e| RepositoryError::query(e.to_string()))?,
                domain::Price::new(Decimal::from_f64_retain(row.try_get::<f64, _>("open").map_err(map_sqlx_error)?).ok_or_else(|| {
                    RepositoryError::serialization("Invalid open price")
                })?)
                .map_err(|e| RepositoryError::query(e.to_string()))?,
                domain::Price::new(Decimal::from_f64_retain(row.try_get::<f64, _>("high").map_err(map_sqlx_error)?).ok_or_else(|| {
                    RepositoryError::serialization("Invalid high price")
                })?)
                .map_err(|e| RepositoryError::query(e.to_string()))?,
                domain::Price::new(Decimal::from_f64_retain(row.try_get::<f64, _>("low").map_err(map_sqlx_error)?).ok_or_else(|| {
                    RepositoryError::serialization("Invalid low price")
                })?)
                .map_err(|e| RepositoryError::query(e.to_string()))?,
                domain::Price::new(Decimal::from_f64_retain(row.try_get::<f64, _>("close").map_err(map_sqlx_error)?).ok_or_else(|| {
                    RepositoryError::serialization("Invalid close price")
                })?)
                .map_err(|e| RepositoryError::query(e.to_string()))?,
                // SAFETY: Volume from database is always non-negative
                unsafe { domain::Volume::new_unchecked(row.try_get::<i64, _>("volume").map_err(map_sqlx_error)?) },
                domain::TimeFrame::M1, // Default timeframe - not stored in DB
                row.try_get::<String, _>("provider").map_err(map_sqlx_error)?,
            )?);
        }

        info!(count = bars.len(), "Latest bars retrieved");
        Ok(bars)
    }
}

/// TimescaleDB implementation of [`TickRepository`]
///
/// Provides optimized storage and retrieval of tick data using TimescaleDB
/// hypertables. Tick data represents individual trades or quote updates
/// and requires efficient time-series storage.
///
/// # Example
///
/// ```rust,no_run
/// use infrastructure::database::timescale::TimescaleTickRepository;
/// use infrastructure::database::DatabasePool;
///
/// async fn example(pool: &DatabasePool) {
///     let repo = TimescaleTickRepository::new(pool);
///     // Use repo to save/query ticks...
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TimescaleTickRepository {
    /// SQLx connection pool
    pool: PgPool,
}

impl TimescaleTickRepository {
    /// Creates a new TimescaleTickRepository
    ///
    /// # Arguments
    ///
    /// * `pool` - The database connection pool
    #[must_use]
    pub fn new(pool: &DatabasePool) -> Self {
        Self {
            pool: pool.pool().clone(),
        }
    }

    /// Executes an operation with retry logic for transient errors
    ///
    /// See [`TimescaleBarRepository::with_retry`] for details.
    async fn with_retry<F, Fut, T>(&self, operation: &str, f: F) -> DomainResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
    {
        let mut delay = Duration::from_millis(INITIAL_RETRY_DELAY_MS);

        for attempt in 1..=MAX_RETRIES {
            match f().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let is_transient = matches!(
                        e,
                        sqlx::Error::PoolTimedOut
                            | sqlx::Error::PoolClosed
                            | sqlx::Error::Io(_)
                            | sqlx::Error::Tls(_)
                    );

                    if !is_transient || attempt == MAX_RETRIES {
                        error!(
                            operation = %operation,
                            attempt = attempt,
                            error = %e,
                            "Database operation failed"
                        );
                        return Err(map_sqlx_error(e));
                    }

                    warn!(
                        operation = %operation,
                        attempt = attempt,
                        error = %e,
                        delay_ms = delay.as_millis(),
                        "Transient error, retrying..."
                    );

                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(
                        delay * 2,
                        Duration::from_millis(MAX_RETRY_DELAY_MS),
                    );
                }
            }
        }

        Err(RepositoryError::connection("Max retries exceeded").into())
    }
}

#[async_trait]
impl TickRepository for TimescaleTickRepository {
    #[instrument(skip(self, tick), fields(symbol = %tick.symbol(), time = %tick.timestamp()))]
    async fn save(&self, tick: &Tick) -> DomainResult<()> {
        debug!("Saving tick to TimescaleDB");

        let symbol = tick.symbol().to_string();
        let time = tick.timestamp();
        let price: f64 = tick.price().inner().try_into().map_err(|_| {
            RepositoryError::serialization("Failed to convert price to f64")
        })?;
        let volume: i64 = tick.size().inner();
        let provider = tick.provider().to_string();

        self.with_retry("save_tick", || {
            let symbol = symbol.clone();
            let provider = provider.clone();
            async move {
                sqlx::query(
                    r#"
                    INSERT INTO ticks (time, symbol, price, volume, provider)
                    VALUES ($1, $2, $3, $4, $5)
                    ON CONFLICT (time, symbol) DO UPDATE SET
                        price = EXCLUDED.price,
                        volume = ticks.volume + EXCLUDED.volume
                    "#,
                )
                .bind(time)
                .bind(&symbol)
                .bind(price)
                .bind(volume)
                .bind(&provider)
                .execute(&self.pool)
                .await
            }
        })
        .await?;

        debug!("Tick saved successfully");
        Ok(())
    }

    #[instrument(skip(self), fields(symbol = %symbol, start = %start, end = %end))]
    async fn get_range(
        &self,
        symbol: &Symbol,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DomainResult<Vec<Tick>> {
        debug!("Querying ticks from TimescaleDB");

        let symbol_str = symbol.to_string();

        let rows: Vec<sqlx::postgres::PgRow> = self
            .with_retry("get_ticks_range", || {
                let symbol_str = symbol_str.clone();
                async move {
                    sqlx::query(
                        r#"
                        SELECT time, symbol, price, volume, provider
                        FROM ticks
                        WHERE symbol = $1 AND time >= $2 AND time <= $3
                        ORDER BY time ASC
                        "#,
                    )
                    .bind(&symbol_str)
                    .bind(start)
                    .bind(end)
                    .fetch_all(&self.pool)
                    .await
                }
            })
            .await?;

        let mut ticks = Vec::with_capacity(rows.len());
        for row in rows {
            ticks.push(Tick::new(
                row.try_get("time").map_err(map_sqlx_error)?,
                Symbol::new(row.try_get::<String, _>("symbol").map_err(map_sqlx_error)?).map_err(|e| RepositoryError::query(e.to_string()))?,
                domain::Price::new(Decimal::from_f64_retain(row.try_get::<f64, _>("price").map_err(map_sqlx_error)?).ok_or_else(|| {
                    RepositoryError::serialization("Invalid price")
                })?)
                .map_err(|e| RepositoryError::query(e.to_string()))?,
                // SAFETY: Volume from database is always non-negative
                unsafe { domain::Volume::new_unchecked(row.try_get::<i64, _>("volume").map_err(map_sqlx_error)?) },
                None, // side not stored in DB
                row.try_get::<String, _>("provider").map_err(map_sqlx_error)?,
            )?);
        }

        info!(count = ticks.len(), "Ticks retrieved");
        Ok(ticks)
    }

    #[instrument(skip(self, ticks), fields(count = ticks.len()))]
    async fn save_batch(&self, ticks: &[Tick]) -> DomainResult<()> {
        if ticks.is_empty() {
            debug!("Empty tick batch, skipping save");
            return Ok(());
        }

        info!(count = ticks.len(), "Saving batch of ticks to TimescaleDB");

        // Prepare vectors for bulk insert using UNNEST
        let mut times: Vec<DateTime<Utc>> = Vec::with_capacity(ticks.len());
        let mut symbols: Vec<String> = Vec::with_capacity(ticks.len());
        let mut prices: Vec<f64> = Vec::with_capacity(ticks.len());
        let mut volumes: Vec<i64> = Vec::with_capacity(ticks.len());
        let mut providers: Vec<String> = Vec::with_capacity(ticks.len());

        for tick in ticks {
            times.push(tick.timestamp());
            symbols.push(tick.symbol().to_string());
            prices.push(
                tick.price()
                    .inner()
                    .try_into()
                    .map_err(|_| RepositoryError::SerializationFailed {
                        reason: "Invalid price".to_string()
                    })?
            );
            volumes.push(tick.size().inner());
            providers.push(tick.provider().to_string());
        }

        self.with_retry("save_tick_batch", || {
            let times = times.clone();
            let symbols = symbols.clone();
            let prices = prices.clone();
            let volumes = volumes.clone();
            let providers = providers.clone();

            async move {
                sqlx::query(
                    r#"
                    INSERT INTO ticks (time, symbol, price, volume, provider)
                    SELECT * FROM UNNEST($1::timestamptz[], $2::text[], $3::float8[], $4::bigint[], $5::text[])
                    ON CONFLICT (time, symbol) DO UPDATE SET
                        price = EXCLUDED.price,
                        volume = ticks.volume + EXCLUDED.volume
                    "#,
                )
                .bind(&times[..])
                .bind(&symbols[..])
                .bind(&prices[..])
                .bind(&volumes[..])
                .bind(&providers[..])
                .execute(&self.pool)
                .await
            }
        })
        .await?;

        info!(count = ticks.len(), "Tick batch saved successfully");
        Ok(())
    }
}

/// Maps a sqlx error to a domain RepositoryError
fn map_sqlx_error(err: sqlx::Error) -> domain::DomainError {
    match err {
        sqlx::Error::RowNotFound => RepositoryError::not_found("Record not found").into(),
        sqlx::Error::PoolTimedOut => RepositoryError::connection("Connection pool timeout").into(),
        sqlx::Error::PoolClosed => RepositoryError::connection("Connection pool closed").into(),
        sqlx::Error::Io(e) => RepositoryError::connection(format!("IO error: {e}")).into(),
        sqlx::Error::Tls(e) => RepositoryError::connection(format!("TLS error: {e}")).into(),
        sqlx::Error::Database(e) => {
            if e.is_unique_violation() {
                RepositoryError::duplicate_key(e.to_string()).into()
            } else if e.is_foreign_key_violation() {
                RepositoryError::constraint_violation(e.to_string()).into()
            } else {
                RepositoryError::query(e.to_string()).into()
            }
        }
        _ => RepositoryError::query(err.to_string()).into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // Note: These tests require a running PostgreSQL/TimescaleDB instance
    // They are marked as ignored by default and should be run in CI with
    // the appropriate database setup.

    #[test]
    fn test_map_sqlx_error_row_not_found() {
        let sqlx_err = sqlx::Error::RowNotFound;
        let domain_err = map_sqlx_error(sqlx_err);

        assert!(matches!(
            domain_err,
            domain::DomainError::Repository(RepositoryError::NotFound { .. })
        ));
    }

    #[test]
    fn test_map_sqlx_error_pool_timeout() {
        let sqlx_err = sqlx::Error::PoolTimedOut;
        let domain_err = map_sqlx_error(sqlx_err);

        assert!(matches!(
            domain_err,
            domain::DomainError::Repository(RepositoryError::ConnectionFailed)
        ));
    }

    #[tokio::test]
    #[ignore = "Requires database connection"]
    async fn test_bar_repository_save_and_get() {
        // This test would require a test database setup
        // It is marked as ignored and should be run in CI
    }

    #[tokio::test]
    #[ignore = "Requires database connection"]
    async fn test_tick_repository_save_and_get() {
        // This test would require a test database setup
        // It is marked as ignored and should be run in CI
    }
}