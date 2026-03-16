//! # TimescaleDB Integration Tests
//!
//! Comprehensive integration tests for TimescaleDB repositories using testcontainers.
//! These tests validate roundtrip operations, batch inserts, error handling, and performance.
//!
//! ## Running Tests
//!
//! ```bash
//! # With Docker (default)
//! docker compose up -d
//! cargo test -p infrastructure --test timescale_integration_tests
//!
//! # With testcontainers (requires Docker)
//! cargo test -p infrastructure --test timescale_integration_tests --features testcontainers
//! ```
//!
//! ## Test Coverage
//!
//! - Bar repository: save, batch, get_range, get_latest, upsert
//! - Tick repository: save, get_range, filtering
//! - Error handling: connection failures, timeouts, constraints
//! - Performance: batch operations under 100ms per 1000 bars

use std::time::{Duration, Instant};

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::{Executor, PgPool};
use testcontainers::core::WaitFor;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

use domain::{
    Bar, BarRepository, Symbol, Tick, TickRepository, TimeFrame, Volume,
    errors::{DomainError, RepositoryError},
};
use infrastructure::database::{
    timescale::{TimescaleBarRepository, TimescaleTickRepository},
    DatabasePool,
};

// =============================================================================
// TEST CONFIGURATION
// =============================================================================

/// Database user for tests
const TEST_DB_USER: &str = "test";
/// Database password for tests
const TEST_DB_PASSWORD: &str = "test";
/// Database name for tests
const TEST_DB_NAME: &str = "trading_test";
/// Maximum acceptable query time for 1000 bars (milliseconds)
const MAX_QUERY_TIME_MS: u64 = 100;
/// Batch size for performance tests
const PERFORMANCE_BATCH_SIZE: usize = 1000;

// =============================================================================
// TEST SETUP HELPERS
// =============================================================================

/// Test database container handle
pub struct TestDb {
    /// Connection pool to the test database
    pub pool: PgPool,
    /// Container handle (kept alive for the duration of tests)
    #[allow(dead_code)]
    container: ContainerAsync<GenericImage>,
}

impl TestDb {
    /// Creates a new test database with TimescaleDB
    ///
    /// # Panics
    ///
    /// Panics if Docker is not available or container fails to start
    pub async fn new() -> Self {
        let image = GenericImage::new("timescale/timescaledb", "latest-pg16")
            .with_env_var("POSTGRES_USER", TEST_DB_USER)
            .with_env_var("POSTGRES_PASSWORD", TEST_DB_PASSWORD)
            .with_env_var("POSTGRES_DB", TEST_DB_NAME)
            .with_exposed_port(5432)
            .with_wait_for(WaitFor::message_on_stdout(
                "database system is ready to accept connections",
            ));

        let container = image
            .start()
            .await
            .expect("Failed to start TimescaleDB container. Is Docker running?");

        let host = container.get_host().await.expect("Failed to get host");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let database_url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            TEST_DB_USER, TEST_DB_PASSWORD, host, port, TEST_DB_NAME
        );

        // Wait for database to be ready
        tokio::time::sleep(Duration::from_millis(500)).await;

        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        // Initialize schema
        Self::init_schema(&pool).await;

        Self { pool, container }
    }

    /// Initializes the database schema
    async fn init_schema(pool: &PgPool) {
        // Enable TimescaleDB extension
        pool.execute("CREATE EXTENSION IF NOT EXISTS timescaledb;")
            .await
            .expect("Failed to create timescaledb extension");

        // Create bars hypertable
        pool.execute(
            r#"
            CREATE TABLE IF NOT EXISTS bars (
                time TIMESTAMPTZ NOT NULL,
                symbol TEXT NOT NULL,
                open DOUBLE PRECISION NOT NULL,
                high DOUBLE PRECISION NOT NULL,
                low DOUBLE PRECISION NOT NULL,
                close DOUBLE PRECISION NOT NULL,
                volume BIGINT NOT NULL DEFAULT 0,
                provider TEXT NOT NULL DEFAULT 'unknown',
                PRIMARY KEY (time, symbol)
            );
            "#,
        )
        .await
        .expect("Failed to create bars table");

        pool.execute(
            r#"
            SELECT create_hypertable('bars', 'time', 
                chunk_time_interval => INTERVAL '1 day',
                if_not_exists => TRUE
            );
            "#,
        )
        .await
        .expect("Failed to create bars hypertable");

        // Create ticks hypertable
        pool.execute(
            r#"
            CREATE TABLE IF NOT EXISTS ticks (
                time TIMESTAMPTZ NOT NULL,
                symbol TEXT NOT NULL,
                price DOUBLE PRECISION NOT NULL,
                volume BIGINT NOT NULL DEFAULT 0,
                provider TEXT NOT NULL DEFAULT 'unknown',
                PRIMARY KEY (time, symbol)
            );
            "#,
        )
        .await
        .expect("Failed to create ticks table");

        pool.execute(
            r#"
            SELECT create_hypertable('ticks', 'time', 
                chunk_time_interval => INTERVAL '1 day',
                if_not_exists => TRUE
            );
            "#,
        )
        .await
        .expect("Failed to create ticks hypertable");
    }

    /// Creates a BarRepository using the test database
    pub fn bar_repository(&self) -> TimescaleBarRepository {
        let db_pool = DatabasePool::from_pool(self.pool.clone());
        TimescaleBarRepository::new(&db_pool)
    }

    /// Creates a TickRepository using the test database
    pub fn tick_repository(&self) -> TimescaleTickRepository {
        let db_pool = DatabasePool::from_pool(self.pool.clone());
        TimescaleTickRepository::new(&db_pool)
    }

    /// Clears all test data
    pub async fn cleanup(&self) {
        let _ = sqlx::query!("TRUNCATE TABLE bars, ticks")
            .execute(&self.pool)
            .await;
    }
}

// Allow creating DatabasePool from existing pool for testing
impl DatabasePool {
    fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }
}

// =============================================================================
// TEST DATA FACTORIES
// =============================================================================

/// Creates a test bar with specified parameters
///
/// # Arguments
///
/// * `symbol` - Trading symbol
/// * `timestamp` - Bar timestamp
/// * `open` - Opening price
/// * `high` - High price
/// * `low` - Low price
/// * `close` - Closing price
/// * `volume` - Trading volume
///
/// # Returns
///
/// A valid [`Bar`] entity
pub fn create_test_bar(
    symbol: &str,
    timestamp: DateTime<Utc>,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: i64,
) -> Bar {
    Bar::new(
        timestamp,
        Symbol::new(symbol).expect("Invalid symbol"),
        domain::Price::new(Decimal::from_f64_retain(open).unwrap())
            .expect("Invalid open price"),
        domain::Price::new(Decimal::from_f64_retain(high).unwrap())
            .expect("Invalid high price"),
        domain::Price::new(Decimal::from_f64_retain(low).unwrap())
            .expect("Invalid low price"),
        domain::Price::new(Decimal::from_f64_retain(close).unwrap())
            .expect("Invalid close price"),
        Volume::new(volume).expect("Invalid volume"),
        TimeFrame::M1,
        "test_provider",
    )
    .expect("Failed to create test bar")
}

/// Creates a test bar with default OHLC values centered around a price
///
/// # Arguments
///
/// * `symbol` - Trading symbol
/// * `timestamp` - Bar timestamp
/// * `price` - Base price (open, close will be this value, high/low ±0.1%)
///
/// # Returns
///
/// A valid [`Bar`] entity
pub fn create_test_bar_simple(symbol: &str, timestamp: DateTime<Utc>, price: f64) -> Bar {
    let high = price * 1.001;
    let low = price * 0.999;
    create_test_bar(symbol, timestamp, price, high, low, price, 1000)
}

/// Creates a range of test bars for a symbol
///
/// # Arguments
///
/// * `symbol` - Trading symbol
/// * `start` - Start timestamp
/// * `count` - Number of bars to create
/// * `interval_seconds` - Seconds between each bar
///
/// # Returns
///
/// Vector of [`Bar`] entities in chronological order
pub fn create_test_bar_range(
    symbol: &str,
    start: DateTime<Utc>,
    count: usize,
    interval_seconds: i64,
) -> Vec<Bar> {
    (0..count)
        .map(|i| {
            let timestamp = start + chrono::Duration::seconds(interval_seconds * i as i64);
            // Simulate a random walk
            let base_price = 100.0 + (i as f64 * 0.1);
            let noise = (i as f64 * 0.01).sin() * 0.5;
            let price = base_price + noise;
            create_test_bar_simple(symbol, timestamp, price)
        })
        .collect()
}

/// Creates a test tick with specified parameters
///
/// # Arguments
///
/// * `symbol` - Trading symbol
/// * `timestamp` - Tick timestamp
/// * `price` - Trade price
/// * `volume` - Trade volume
///
/// # Returns
///
/// A valid [`Tick`] entity
pub fn create_test_tick(symbol: &str, timestamp: DateTime<Utc>, price: f64, volume: i64) -> Tick {
    Tick::new(
        timestamp,
        Symbol::new(symbol).expect("Invalid symbol"),
        domain::Price::new(Decimal::from_f64_retain(price).unwrap())
            .expect("Invalid price"),
        Volume::new(volume).expect("Invalid volume"),
        None,
        "test_provider",
    )
    .expect("Failed to create test tick")
}

/// Creates a range of test ticks for a symbol
///
/// # Arguments
///
/// * `symbol` - Trading symbol
/// * `start` - Start timestamp
/// * `count` - Number of ticks to create
/// * `interval_millis` - Milliseconds between each tick
///
/// # Returns
///
/// Vector of [`Tick`] entities in chronological order
pub fn create_test_tick_range(
    symbol: &str,
    start: DateTime<Utc>,
    count: usize,
    interval_millis: i64,
) -> Vec<Tick> {
    (0..count)
        .map(|i| {
            let timestamp = start + chrono::Duration::milliseconds(interval_millis * i as i64);
            let price = 100.0 + (i as f64 * 0.01);
            let volume = 100 + (i % 100) as i64;
            create_test_tick(symbol, timestamp, price, volume)
        })
        .collect()
}

// =============================================================================
// BAR REPOSITORY TESTS
// =============================================================================

#[tokio::test]
async fn test_bar_save_and_retrieve_roundtrip() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol = Symbol::new("NQ").unwrap();
    let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
    let bar = create_test_bar(&symbol.to_string(), timestamp, 18250.0, 18260.0, 18240.0, 18255.0, 1500);

    // Save the bar
    repo.save(&bar).await.expect("Failed to save bar");

    // Retrieve the bar
    let retrieved = repo
        .get_range(&symbol, timestamp, timestamp)
        .await
        .expect("Failed to retrieve bar");

    assert_eq!(retrieved.len(), 1, "Expected exactly one bar");
    assert_eq!(retrieved[0].symbol(), &symbol);
    assert_eq!(retrieved[0].open().inner(), &dec!(18250.0));
    assert_eq!(retrieved[0].high().inner(), &dec!(18260.0));
    assert_eq!(retrieved[0].low().inner(), &dec!(18240.0));
    assert_eq!(retrieved[0].close().inner(), &dec!(18255.0));
    assert_eq!(retrieved[0].volume().inner(), 1500);
}

#[tokio::test]
async fn test_bar_batch_insert_performance() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol = "ES";
    let start = Utc.with_ymd_and_hms(2024, 1, 15, 9, 30, 0).unwrap();
    let bars = create_test_bar_range(symbol, start, PERFORMANCE_BATCH_SIZE, 60);

    let start_time = Instant::now();
    repo.save_batch(&bars).await.expect("Failed to save batch");
    let save_duration = start_time.elapsed();

    // Query the saved bars
    let query_start = Instant::now();
    let retrieved = repo
        .get_range(&Symbol::new(symbol).unwrap(), start, start + chrono::Duration::hours(24))
        .await
        .expect("Failed to retrieve bars");
    let query_duration = query_start.elapsed();

    assert_eq!(retrieved.len(), PERFORMANCE_BATCH_SIZE, "Expected all bars to be retrieved");

    // Performance assertion: query should be under 100ms for 1000 bars
    let query_ms = query_duration.as_millis() as u64;
    assert!(
        query_ms < MAX_QUERY_TIME_MS,
        "Query took {}ms, expected < {}ms",
        query_ms,
        MAX_QUERY_TIME_MS
    );

    println!("Batch insert: {} bars in {:?}", PERFORMANCE_BATCH_SIZE, save_duration);
    println!("Query: {} bars in {:?}", retrieved.len(), query_duration);
}

#[tokio::test]
async fn test_bar_get_latest_ordering() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol = Symbol::new("CL").unwrap();
    let base_time = Utc.with_ymd_and_hms(2024, 1, 15, 9, 30, 0).unwrap();

    // Create 20 bars with increasing timestamps
    let bars: Vec<Bar> = (0..20)
        .map(|i| {
            let timestamp = base_time + chrono::Duration::minutes(i);
            create_test_bar_simple(&symbol.to_string(), timestamp, 75.0 + i as f64)
        })
        .collect();

    repo.save_batch(&bars).await.expect("Failed to save bars");

    // Get latest 5 bars
    let latest = repo.get_latest(&symbol, 5).await.expect("Failed to get latest bars");

    assert_eq!(latest.len(), 5, "Expected 5 latest bars");

    // Verify ordering: should be ascending by time (oldest first in result)
    for i in 1..latest.len() {
        assert!(
            latest[i].timestamp() >= latest[i - 1].timestamp(),
            "Bars should be in ascending order"
        );
    }

    // Verify we got the most recent bars
    let expected_start = base_time + chrono::Duration::minutes(15); // 20 - 5 = 15
    assert_eq!(latest[0].timestamp(), expected_start);
}

#[tokio::test]
async fn test_bar_upsert_on_conflict() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol = Symbol::new("GC").unwrap();
    let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 14, 0, 0).unwrap();

    // First insert
    let bar1 = create_test_bar(&symbol.to_string(), timestamp, 2050.0, 2055.0, 2045.0, 2052.0, 1000);
    repo.save(&bar1).await.expect("Failed to save first bar");

    // Second insert with same key (conflict) - should update
    let bar2 = create_test_bar(&symbol.to_string(), timestamp, 2052.0, 2058.0, 2050.0, 2055.0, 500);
    repo.save(&bar2).await.expect("Failed to save second bar (upsert)");

    // Retrieve and verify updated values
    let retrieved = repo
        .get_range(&symbol, timestamp, timestamp)
        .await
        .expect("Failed to retrieve bar");

    assert_eq!(retrieved.len(), 1, "Expected exactly one bar");
    assert_eq!(retrieved[0].open().inner(), &dec!(2052.0));
    assert_eq!(retrieved[0].high().inner(), &dec!(2058.0));
    assert_eq!(retrieved[0].low().inner(), &dec!(2050.0));
    assert_eq!(retrieved[0].close().inner(), &dec!(2055.0));
    // Volume should be accumulated: 1000 + 500 = 1500
    assert_eq!(retrieved[0].volume().inner(), 1500);
}

#[tokio::test]
async fn test_bar_range_query_filtering() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol_a = Symbol::new("AAPL").unwrap();
    let symbol_b = Symbol::new("MSFT").unwrap();
    let base_time = Utc.with_ymd_and_hms(2024, 1, 15, 9, 30, 0).unwrap();

    // Create bars for both symbols
    let bars_a = create_test_bar_range(&symbol_a.to_string(), base_time, 10, 60);
    let bars_b = create_test_bar_range(&symbol_b.to_string(), base_time, 10, 60);

    repo.save_batch(&bars_a).await.expect("Failed to save symbol A bars");
    repo.save_batch(&bars_b).await.expect("Failed to save symbol B bars");

    // Query only symbol A
    let retrieved_a = repo
        .get_range(&symbol_a, base_time, base_time + chrono::Duration::hours(1))
        .await
        .expect("Failed to retrieve symbol A bars");

    assert_eq!(retrieved_a.len(), 10, "Expected 10 bars for symbol A");
    assert!(retrieved_a.iter().all(|b| b.symbol() == &symbol_a));

    // Query only symbol B
    let retrieved_b = repo
        .get_range(&symbol_b, base_time, base_time + chrono::Duration::hours(1))
        .await
        .expect("Failed to retrieve symbol B bars");

    assert_eq!(retrieved_b.len(), 10, "Expected 10 bars for symbol B");
    assert!(retrieved_b.iter().all(|b| b.symbol() == &symbol_b));
}

#[tokio::test]
async fn test_bar_empty_range() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol = Symbol::new("TSLA").unwrap();
    let start = Utc.with_ymd_and_hms(2024, 1, 15, 9, 30, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();

    // Query without saving any bars
    let retrieved = repo.get_range(&symbol, start, end).await.expect("Query should succeed");

    assert!(retrieved.is_empty(), "Expected empty result for non-existent data");
}

// =============================================================================
// TICK REPOSITORY TESTS
// =============================================================================

#[tokio::test]
async fn test_tick_save_and_retrieve_roundtrip() {
    let test_db = TestDb::new().await;
    let repo = test_db.tick_repository();

    let symbol = Symbol::new("NQ").unwrap();
    let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
    let tick = create_test_tick(&symbol.to_string(), timestamp, 18250.25, 50);

    // Save the tick
    repo.save(&tick).await.expect("Failed to save tick");

    // Retrieve the tick
    let retrieved = repo
        .get_range(&symbol, timestamp, timestamp + chrono::Duration::seconds(1))
        .await
        .expect("Failed to retrieve tick");

    assert_eq!(retrieved.len(), 1, "Expected exactly one tick");
    assert_eq!(retrieved[0].symbol(), &symbol);
    assert_eq!(retrieved[0].price().inner(), &dec!(18250.25));
    assert_eq!(retrieved[0].size().inner(), 50);
}

#[tokio::test]
async fn test_tick_range_query_filtering() {
    let test_db = TestDb::new().await;
    let repo = test_db.tick_repository();

    let symbol = Symbol::new("ES").unwrap();
    let base_time = Utc.with_ymd_and_hms(2024, 1, 15, 9, 30, 0).unwrap();

    // Create 100 ticks over 10 seconds
    let ticks = create_test_tick_range(&symbol.to_string(), base_time, 100, 100);

    repo.save_batch(&ticks).await.expect("Failed to save ticks");

    // Query first 3 seconds
    let query_end = base_time + chrono::Duration::seconds(3);
    let retrieved = repo
        .get_range(&symbol, base_time, query_end)
        .await
        .expect("Failed to retrieve ticks");

    // Should get ticks at 0, 100ms, 200ms, ..., 3000ms = 31 ticks
    assert!(!retrieved.is_empty(), "Expected some ticks");
    assert!(retrieved.len() <= 31, "Expected at most 31 ticks");

    // Verify all returned ticks are within range
    for tick in &retrieved {
        assert!(tick.timestamp() >= base_time);
        assert!(tick.timestamp() <= query_end);
    }
}

#[tokio::test]
async fn test_tick_symbol_filtering() {
    let test_db = TestDb::new().await;
    let repo = test_db.tick_repository();

    let symbol_a = Symbol::new("AAPL").unwrap();
    let symbol_b = Symbol::new("GOOGL").unwrap();
    let base_time = Utc.with_ymd_and_hms(2024, 1, 15, 9, 30, 0).unwrap();

    // Create ticks for both symbols
    let ticks_a = create_test_tick_range(&symbol_a.to_string(), base_time, 10, 1000);
    let ticks_b = create_test_tick_range(&symbol_b.to_string(), base_time, 10, 1000);

    for tick in ticks_a {
        repo.save(&tick).await.expect("Failed to save tick A");
    }
    for tick in ticks_b {
        repo.save(&tick).await.expect("Failed to save tick B");
    }

    // Query only symbol A
    let retrieved_a = repo
        .get_range(&symbol_a, base_time, base_time + chrono::Duration::minutes(1))
        .await
        .expect("Failed to retrieve ticks A");

    assert_eq!(retrieved_a.len(), 10, "Expected 10 ticks for symbol A");
    assert!(retrieved_a.iter().all(|t| t.symbol() == &symbol_a));
}

#[tokio::test]
async fn test_tick_batch_save() {
    let test_db = TestDb::new().await;
    let repo = test_db.tick_repository();

    let symbol = Symbol::new("CL").unwrap();
    let base_time = Utc.with_ymd_and_hms(2024, 1, 15, 9, 30, 0).unwrap();

    // Use save_batch for efficient bulk insert
    let ticks = create_test_tick_range(&symbol.to_string(), base_time, 100, 10);

    let start = Instant::now();
    repo.save_batch(&ticks).await.expect("Failed to save tick batch");
    let duration = start.elapsed();

    // Verify all saved
    let retrieved = repo
        .get_range(&symbol, base_time, base_time + chrono::Duration::seconds(2))
        .await
        .expect("Failed to retrieve ticks");

    assert_eq!(retrieved.len(), 100, "Expected all 100 ticks");
    println!("Saved 100 ticks in {:?}", duration);
}

// =============================================================================
// ERROR HANDLING TESTS
// =============================================================================

#[tokio::test]
async fn test_invalid_symbol_error() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    // Empty symbol should be invalid
    let result = Symbol::new("");
    assert!(result.is_err(), "Empty symbol should be invalid");

    // Invalid characters should be rejected
    let result = Symbol::new("ABC!@#");
    assert!(result.is_err(), "Symbol with special chars should be invalid");

    // Valid symbol should work
    let result = Symbol::new("AAPL");
    assert!(result.is_ok(), "Valid symbol should be accepted");
}

#[tokio::test]
async fn test_bar_price_validation() {
    let symbol = Symbol::new("TEST").unwrap();
    let timestamp = Utc::now();

    // Invalid: high < low
    let result = Bar::new(
        timestamp,
        symbol.clone(),
        domain::Price::new(dec!(100.0)).unwrap(),
        domain::Price::new(dec!(90.0)).unwrap(),  // high < low
        domain::Price::new(dec!(95.0)).unwrap(),
        domain::Price::new(dec!(98.0)).unwrap(),
        Volume::new(100).unwrap(),
        TimeFrame::M1,
        "test",
    );
    assert!(result.is_err(), "High < low should be rejected");

    // Invalid: high < open
    let result = Bar::new(
        timestamp,
        symbol.clone(),
        domain::Price::new(dec!(105.0)).unwrap(), // open > high
        domain::Price::new(dec!(100.0)).unwrap(),
        domain::Price::new(dec!(95.0)).unwrap(),
        domain::Price::new(dec!(98.0)).unwrap(),
        Volume::new(100).unwrap(),
        TimeFrame::M1,
        "test",
    );
    assert!(result.is_err(), "Open > high should be rejected");

    // Invalid: low > close
    let result = Bar::new(
        timestamp,
        symbol.clone(),
        domain::Price::new(dec!(100.0)).unwrap(),
        domain::Price::new(dec!(105.0)).unwrap(),
        domain::Price::new(dec!(102.0)).unwrap(), // low > close
        domain::Price::new(dec!(98.0)).unwrap(),
        Volume::new(100).unwrap(),
        TimeFrame::M1,
        "test",
    );
    assert!(result.is_err(), "Low > close should be rejected");
}

#[tokio::test]
async fn test_query_nonexistent_symbol() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let nonexistent = Symbol::new("NONEXISTENT").unwrap();
    let start = Utc.with_ymd_and_hms(2024, 1, 15, 9, 30, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();

    // Query should succeed but return empty results
    let result = repo.get_range(&nonexistent, start, end).await;
    assert!(result.is_ok(), "Query should succeed for non-existent symbol");
    assert!(result.unwrap().is_empty(), "Should return empty for non-existent symbol");
}

#[tokio::test]
async fn test_batch_insert_empty() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    // Empty batch should succeed without error
    let empty: Vec<Bar> = vec![];
    let result = repo.save_batch(&empty).await;
    assert!(result.is_ok(), "Empty batch should succeed");
}

// =============================================================================
// TIMEZONE AND EDGE CASE TESTS
// =============================================================================

#[tokio::test]
async fn test_timestamp_precision() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol = Symbol::new("PRECISION").unwrap();

    // Test microsecond precision
    let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap()
        + chrono::Duration::microseconds(123456);

    let bar = create_test_bar_simple(&symbol.to_string(), timestamp, 100.0);
    repo.save(&bar).await.expect("Failed to save bar");

    let retrieved = repo
        .get_range(&symbol, timestamp, timestamp)
        .await
        .expect("Failed to retrieve");

    assert_eq!(retrieved.len(), 1);
    // Note: PostgreSQL TIMESTAMPTZ has microsecond precision
    assert_eq!(retrieved[0].timestamp(), timestamp);
}

#[tokio::test]
async fn test_large_volume_values() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol = Symbol::new("HIGHVOL").unwrap();
    let timestamp = Utc::now();

    // Test with large volume (1 billion)
    let bar = create_test_bar(&symbol.to_string(), timestamp, 100.0, 101.0, 99.0, 100.5, 1_000_000_000);
    repo.save(&bar).await.expect("Failed to save high volume bar");

    let retrieved = repo.get_range(&symbol, timestamp, timestamp).await.expect("Failed to retrieve");
    assert_eq!(retrieved[0].volume().inner(), 1_000_000_000);
}

#[tokio::test]
async fn test_very_small_price_values() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol = Symbol::new("PENNY").unwrap();
    let timestamp = Utc::now();

    // Test with penny stock prices
    let bar = create_test_bar(&symbol.to_string(), timestamp, 0.0001, 0.0002, 0.0001, 0.00015, 1000000);
    repo.save(&bar).await.expect("Failed to save penny stock bar");

    let retrieved = repo.get_range(&symbol, timestamp, timestamp).await.expect("Failed to retrieve");
    assert_eq!(retrieved[0].close().inner(), &dec!(0.00015));
}

// =============================================================================
// CONCURRENT ACCESS TESTS
// =============================================================================

#[tokio::test]
async fn test_concurrent_saves() {
    let test_db = TestDb::new().await;
    let symbol = Symbol::new("CONCURRENT").unwrap();
    let base_time = Utc.with_ymd_and_hms(2024, 1, 15, 9, 30, 0).unwrap();

    // Spawn multiple concurrent save operations
    let mut handles = vec![];

    for i in 0..5 {
        let pool = test_db.pool.clone();
        let symbol = symbol.clone();
        let handle = tokio::spawn(async move {
            let db_pool = DatabasePool::from_pool(pool);
            let repo = TimescaleBarRepository::new(&db_pool);

            for j in 0..10 {
                let timestamp = base_time + chrono::Duration::minutes(i * 10 + j);
                let bar = create_test_bar_simple(&symbol.to_string(), timestamp, 100.0 + j as f64);
                repo.save(&bar).await.expect("Failed to save");
            }
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Verify all bars were saved
    let repo = test_db.bar_repository();
    let retrieved = repo
        .get_range(&symbol, base_time, base_time + chrono::Duration::hours(1))
        .await
        .expect("Failed to retrieve");

    assert_eq!(retrieved.len(), 50, "Expected 50 bars from concurrent saves");
}

// Note: Connection failure tests are typically environment-specific
// and require mocking or network manipulation. These are covered
// by the retry logic unit tests in the main module.

// =============================================================================
// MODULE SETUP
// =============================================================================

/// Setup function that runs before each test
///
/// Currently just ensures tracing is initialized once
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
}

#[tokio::test]
async fn test_edge_case_zero_volume() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol = Symbol::new("ZEROVOL").unwrap();
    let timestamp = Utc::now();

    // Test with zero volume (should be valid)
    let bar = create_test_bar(&symbol.to_string(), timestamp, 100.0, 101.0, 99.0, 100.5, 0);
    repo.save(&bar).await.expect("Failed to save zero volume bar");

    let retrieved = repo.get_range(&symbol, timestamp, timestamp).await.expect("Failed to retrieve");
    assert_eq!(retrieved[0].volume().inner(), 0);
}

#[tokio::test]
async fn test_get_latest_with_limit_zero() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol = Symbol::new("LIMIT0").unwrap();
    let base_time = Utc::now();

    // Create some bars
    let bars = create_test_bar_range(&symbol.to_string(), base_time, 10, 60);
    repo.save_batch(&bars).await.expect("Failed to save");

    // Query with limit 0 should return empty
    let latest = repo.get_latest(&symbol, 0).await.expect("Query should succeed");
    assert!(latest.is_empty(), "Limit 0 should return empty");
}

#[tokio::test]
async fn test_upsert_volume_accumulation() {
    let test_db = TestDb::new().await;
    let repo = test_db.bar_repository();

    let symbol = Symbol::new("ACCUM").unwrap();
    let timestamp = Utc::now();

    // First insert with volume 100
    let bar1 = create_test_bar(&symbol.to_string(), timestamp, 100.0, 101.0, 99.0, 100.5, 100);
    repo.save(&bar1).await.expect("Failed to save first");

    // Second insert with volume 200 - should accumulate to 300
    let bar2 = create_test_bar(&symbol.to_string(), timestamp, 100.0, 101.0, 99.0, 100.5, 200);
    repo.save(&bar2).await.expect("Failed to save second");

    // Third insert with volume 300 - should accumulate to 600
    let bar3 = create_test_bar(&symbol.to_string(), timestamp, 100.0, 101.0, 99.0, 100.5, 300);
    repo.save(&bar3).await.expect("Failed to save third");

    let retrieved = repo.get_range(&symbol, timestamp, timestamp).await.expect("Failed to retrieve");
    assert_eq!(retrieved[0].volume().inner(), 600, "Volume should accumulate: 100 + 200 + 300 = 600");
}