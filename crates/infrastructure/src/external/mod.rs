//! # External Module
//!
//! External API clients and adapters (broker APIs, market data providers).

pub mod ib;

use async_trait::async_trait;
use reqwest::Client;

use application::errors::{ApplicationError, ApplicationResult};
use application::ports::{MarketDataPort, NotificationLevel, NotificationPort, OrderExecutionPort};

/// HTTP client for external APIs
#[derive(Debug, Clone)]
pub struct HttpClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl HttpClient {
    /// Creates a new HTTP client
    #[must_use]
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            api_key,
        }
    }

    /// Returns the underlying reqwest client
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Returns the base URL
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the API key if present
    #[must_use]
    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }
}

/// Mock order execution adapter for development
pub struct MockOrderExecutionAdapter {
    client: HttpClient,
}

impl MockOrderExecutionAdapter {
    /// Creates a new MockOrderExecutionAdapter
    #[must_use]
    pub fn new(client: HttpClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl OrderExecutionPort for MockOrderExecutionAdapter {
    async fn submit_order(&self, order_id: uuid::Uuid) -> ApplicationResult<()> {
        tracing::info!("Mock: Submitting order {}", order_id);
        Ok(())
    }

    async fn cancel_order(&self, order_id: uuid::Uuid) -> ApplicationResult<()> {
        tracing::info!("Mock: Cancelling order {}", order_id);
        Ok(())
    }

    async fn is_market_open(&self, symbol: &str) -> ApplicationResult<bool> {
        tracing::info!("Mock: Checking market status for {}", symbol);
        Ok(true)
    }
}

/// Mock market data adapter for development
pub struct MockMarketDataAdapter {
    client: HttpClient,
}

impl MockMarketDataAdapter {
    /// Creates a new MockMarketDataAdapter
    #[must_use]
    pub fn new(client: HttpClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl MarketDataPort for MockMarketDataAdapter {
    async fn get_current_price(&self, symbol: &str) -> ApplicationResult<rust_decimal::Decimal> {
        tracing::info!("Mock: Getting current price for {}", symbol);
        // Return mock price
        Ok(rust_decimal::Decimal::from(100))
    }

    async fn is_market_open(&self, _symbol: &str) -> ApplicationResult<bool> {
        Ok(true) // Always open for mock
    }

    async fn get_historical_prices(
        &self,
        symbol: &str,
        _start: chrono::DateTime<chrono::Utc>,
        _end: chrono::DateTime<chrono::Utc>,
    ) -> ApplicationResult<Vec<(chrono::DateTime<chrono::Utc>, rust_decimal::Decimal)>> {
        tracing::info!("Mock: Getting historical prices for {}", symbol);
        Ok(vec![])
    }
}

/// Console notification adapter
pub struct ConsoleNotificationAdapter;

impl ConsoleNotificationAdapter {
    /// Creates a new ConsoleNotificationAdapter
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for ConsoleNotificationAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NotificationPort for ConsoleNotificationAdapter {
    async fn notify(&self, message: &str, level: NotificationLevel) -> ApplicationResult<()> {
        match level {
            NotificationLevel::Info => tracing::info!("[NOTIFICATION] {}", message),
            NotificationLevel::Warning => tracing::warn!("[NOTIFICATION] {}", message),
            NotificationLevel::Error => tracing::error!("[NOTIFICATION] {}", message),
            NotificationLevel::Critical => tracing::error!("[CRITICAL] {}", message),
            _ => tracing::info!("[NOTIFICATION] {}", message), // Fallback for non-exhaustive
        }
        Ok(())
    }
}
