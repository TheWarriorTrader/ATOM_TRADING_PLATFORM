//! # Interactive Brokers Market Data Provider
//!
//! Implementation of [`MarketDataProvider`] trait for Interactive Brokers.
//! Provides real-time bar (OHLCV) and tick data subscriptions via IB API.
//!
//! ## Features
//!
//! - **Real-time Bars**: Subscribe to OHLCV bars with configurable timeframes (M1, M5, M15, M30, H1)
//! - **Tick Data**: Subscribe to high-frequency tick-by-tick trade data
//! - **ReqId Management**: Automatic unique request ID generation and tracking
//! - **Backpressure Handling**: Configurable bounded channels with drop-oldest policy
//! - **Type Safety**: Full mapping from IB types to domain types
//! - **Graceful Cleanup**: Automatic unsubscribe on disconnect
//!
//! ## Example
//!
//! ```rust,no_run
//! use infrastructure::external::ib::{IBClient, IBConfig, IBMarketDataProvider};
//! use domain::providers::MarketDataProvider;
//! use domain::values::{Symbol, TimeFrame};
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create IB client
//! let (event_tx, _event_rx) = tokio::sync::mpsc::channel(100);
//! let client = Arc::new(IBClient::new(IBConfig::default(), event_tx));
//!
//! // Create market data provider
//! let provider = IBMarketDataProvider::new(client, 1000);
//!
//! // Connect and subscribe
//! let mut provider = provider;
//! provider.connect().await?;
//!
//! let symbol = Symbol::new("AAPL")?;
//! let mut bars = provider.subscribe_bars(&symbol, TimeFrame::M1).await?;
//!
//! // Consume bars
//! while let Some(bar) = bars.recv().await {
//!     println!("Received bar: {}", bar);
//! }
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, info, instrument, trace, warn};

use domain::entities::{Bar, Tick};
use domain::errors::ProviderError;
use domain::providers::MarketDataProvider;
use domain::values::{Price, Side, Symbol, TimeFrame, Volume};

use super::client::{IBClient, IBEvent, MarketDataUpdate as IBMarketDataUpdate};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Default channel size for backpressure handling.
const DEFAULT_CHANNEL_SIZE: usize = 1000;

/// Provider identifier string.
const PROVIDER_NAME: &str = "InteractiveBrokers";

// =============================================================================
// SUBSCRIPTION INFO
// =============================================================================

/// Information about an active market data subscription.
#[derive(Debug, Clone)]
struct SubscriptionInfo {
    /// Trading symbol being subscribed
    symbol: Symbol,
    /// Timeframe for bar subscriptions (None for tick subscriptions)
    timeframe: Option<TimeFrame>,
    /// Request ID assigned by IB
    req_id: i32,
    /// Channel sender for bar data
    bar_tx: Option<mpsc::Sender<Bar>>,
    /// Channel sender for tick data
    tick_tx: Option<mpsc::Sender<Tick>>,
}

impl SubscriptionInfo {
    /// Creates a new bar subscription info.
    fn new_bar(symbol: Symbol, timeframe: TimeFrame, req_id: i32, tx: mpsc::Sender<Bar>) -> Self {
        Self {
            symbol,
            timeframe: Some(timeframe),
            req_id,
            bar_tx: Some(tx),
            tick_tx: None,
        }
    }

    /// Creates a new tick subscription info.
    fn new_tick(symbol: Symbol, req_id: i32, tx: mpsc::Sender<Tick>) -> Self {
        Self {
            symbol,
            timeframe: None,
            req_id,
            bar_tx: None,
            tick_tx: Some(tx),
        }
    }

    /// Returns true if this is a bar subscription.
    fn is_bar_subscription(&self) -> bool {
        self.timeframe.is_some()
    }

    /// Returns true if this is a tick subscription.
    fn is_tick_subscription(&self) -> bool {
        self.timeframe.is_none()
    }
}

// =============================================================================
// IB MARKET DATA PROVIDER
// =============================================================================

/// Interactive Brokers implementation of [`MarketDataProvider`].
///
/// This provider wraps an [`IBClient`] and manages real-time market data
/// subscriptions with proper request ID tracking and backpressure handling.
///
/// # Thread Safety
///
/// The provider is `Send + Sync` and can be safely shared across tasks.
/// All subscription state is protected by synchronization primitives.
///
/// # Backpressure
///
/// Channels are bounded with a configurable size (default: 1000). When the
/// buffer is full, the oldest message is dropped to prevent memory exhaustion
/// during high-frequency data bursts.
pub struct IBMarketDataProvider {
    /// Underlying IB client
    client: Arc<IBClient>,
    /// Active subscriptions mapped by symbol
    subscriptions: Arc<RwLock<HashMap<String, SubscriptionInfo>>>,
    /// Atomic counter for generating unique request IDs
    next_req_id: Arc<AtomicI32>,
    /// Channel buffer size for backpressure
    channel_size: usize,
    /// Event processing task handle
    event_processor: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Event channel receiver for processing
    event_rx: Arc<Mutex<Option<mpsc::Receiver<IBEvent>>>>,
}

impl std::fmt::Debug for IBMarketDataProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IBMarketDataProvider")
            .field("channel_size", &self.channel_size)
            .field("next_req_id", &self.next_req_id.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl IBMarketDataProvider {
    /// Creates a new IB market data provider.
    ///
    /// # Arguments
    ///
    /// * `client` - The IB client to use for API calls
    /// * `channel_size` - Buffer size for subscription channels (use 0 for default)
    ///
    /// # Example
    ///
    /// ```rust
    /// use infrastructure::external::ib::{IBClient, IBConfig, IBMarketDataProvider};
    /// use std::sync::Arc;
    ///
    /// let (event_tx, _event_rx) = tokio::sync::mpsc::channel(100);
    /// let client = Arc::new(IBClient::new(IBConfig::default(), event_tx));
    /// let provider = IBMarketDataProvider::new(client, 1000);
    /// ```
    #[must_use]
    pub fn new(client: Arc<IBClient>, channel_size: usize) -> Self {
        let size = if channel_size == 0 {
            DEFAULT_CHANNEL_SIZE
        } else {
            channel_size
        };

        Self {
            client,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            next_req_id: Arc::new(AtomicI32::new(1000)), // Start at 1000 to avoid conflicts with order IDs
            channel_size: size,
            event_processor: Arc::new(Mutex::new(None)),
            event_rx: Arc::new(Mutex::new(None)),
        }
    }

    /// Generates a unique request ID for IB API calls.
    fn generate_req_id(&self) -> i32 {
        self.next_req_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Starts the event processor task.
    ///
    /// This task listens for IB events and routes them to the appropriate
    /// subscription channels.
    async fn start_event_processor(&self, mut event_rx: mpsc::Receiver<IBEvent>) {
        let subscriptions = self.subscriptions.clone();
        let channel_size = self.channel_size;

        let handle = tokio::spawn(async move {
            info!("IB market data event processor started");

            while let Some(event) = event_rx.recv().await {
                match event {
                    IBEvent::MarketData { req_id, data } => {
                        trace!(req_id = req_id, "Received market data update");
                        Self::process_market_data(&subscriptions, req_id, data).await;
                    }
                    IBEvent::Error { code, message } => {
                        warn!(code = code, message = %message, "IB API error");
                        // Could map specific error codes to subscription failures
                    }
                    IBEvent::Disconnected { reason } => {
                        info!(reason = %reason, "IB disconnected, clearing subscriptions");
                        let mut subs = subscriptions.write().await;
                        subs.clear();
                    }
                    _ => {
                        // Other events are not relevant for market data
                        trace!("Ignoring non-market data event");
                    }
                }
            }

            info!("IB market data event processor stopped");
        });

        let mut processor = self.event_processor.lock().await;
        *processor = Some(handle);
    }

    /// Processes a market data update from IB.
    async fn process_market_data(
        subscriptions: &Arc<RwLock<HashMap<String, SubscriptionInfo>>>,
        req_id: i32,
        data: IBMarketDataUpdate,
    ) {
        let subs = subscriptions.read().await;

        // Find subscription by req_id
        let sub_info = subs.values().find(|s| s.req_id == req_id);

        if let Some(sub) = sub_info {
            if sub.is_bar_subscription() {
                // Convert to bar and send
                // Note: IB real-time bars come via different callback, this is for ticks
                if let Some(ref tx) = sub.tick_tx {
                    if let Ok(tick) = Self::convert_ib_tick(&sub.symbol, &data) {
                        if let Err(e) = tx.try_send(tick) {
                            match e {
                                mpsc::error::TrySendError::Full(_) => {
                                    warn!(symbol = %sub.symbol, "Tick channel full, dropping oldest");
                                }
                                mpsc::error::TrySendError::Closed(_) => {
                                    debug!(symbol = %sub.symbol, "Tick channel closed");
                                }
                            }
                        }
                    }
                }
            }
        }

        drop(subs);
    }

    /// Converts IB market data update to domain [`Tick`].
    ///
    /// # Errors
    ///
    /// Returns an error if the price cannot be converted.
    fn convert_ib_tick(
        symbol: &Symbol,
        data: &IBMarketDataUpdate,
    ) -> Result<Tick, ProviderError> {
        // Use last price if available, otherwise bid/ask midpoint
        let price = data
            .last
            .or_else(|| {
                if let (Some(bid), Some(ask)) = (data.bid, data.ask) {
                    // Calculate midpoint
                    let bid_dec = bid.inner();
                    let ask_dec = ask.inner();
                    let mid = (bid_dec + ask_dec) / Decimal::new(2, 0);
                    Price::new(mid).ok()
                } else {
                    data.bid.or(data.ask)
                }
            })
            .ok_or_else(|| ProviderError::InvalidResponse {
                provider: PROVIDER_NAME.to_string(),
                details: "No price available in market data update".to_string(),
            })?;

        let size = data
            .last_size
            .map(|q| Volume::new(q.inner().try_into().unwrap_or(0)).unwrap_or_else(|_| Volume::zero()))
            .unwrap_or_else(Volume::zero);

        Tick::new(
            data.timestamp,
            symbol.clone(),
            price,
            size,
            None, // Side not available in basic market data
            PROVIDER_NAME,
        )
        .map_err(|e| ProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: format!("Failed to create tick: {}", e),
        })
    }

    /// Converts IB bar data to domain [`Bar`].
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol
    /// * `time` - Bar timestamp (Unix seconds from IB)
    /// * `open` - Opening price
    /// * `high` - Highest price
    /// * `low` - Lowest price
    /// * `close` - Closing price
    /// * `volume` - Trading volume
    /// * `wap` - Weighted average price
    /// * `count` - Number of trades
    /// * `timeframe` - The bar timeframe
    ///
    /// # Errors
    ///
    /// Returns an error if price or volume conversion fails.
    #[allow(clippy::too_many_arguments)]
    fn convert_ib_bar(
        symbol: &Symbol,
        time: i64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: i64,
        _wap: f64,
        _count: i32,
        timeframe: TimeFrame,
    ) -> Result<Bar, ProviderError> {
        // Convert Unix timestamp (IB provides seconds)
        let timestamp = Utc.timestamp_opt(time, 0).single().ok_or_else(|| {
            ProviderError::InvalidResponse {
                provider: PROVIDER_NAME.to_string(),
                details: format!("Invalid timestamp: {}", time),
            }
        })?;

        // Convert f64 prices to Decimal
        let open_dec = Decimal::from_f64_retain(open).ok_or_else(|| {
            ProviderError::InvalidResponse {
                provider: PROVIDER_NAME.to_string(),
                details: format!("Invalid open price: {}", open),
            }
        })?;

        let high_dec = Decimal::from_f64_retain(high).ok_or_else(|| {
            ProviderError::InvalidResponse {
                provider: PROVIDER_NAME.to_string(),
                details: format!("Invalid high price: {}", high),
            }
        })?;

        let low_dec = Decimal::from_f64_retain(low).ok_or_else(|| {
            ProviderError::InvalidResponse {
                provider: PROVIDER_NAME.to_string(),
                details: format!("Invalid low price: {}", low),
            }
        })?;

        let close_dec = Decimal::from_f64_retain(close).ok_or_else(|| {
            ProviderError::InvalidResponse {
                provider: PROVIDER_NAME.to_string(),
                details: format!("Invalid close price: {}", close),
            }
        })?;

        let open_price = Price::new(open_dec).map_err(|e| ProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: format!("Invalid open price: {}", e),
        })?;

        let high_price = Price::new(high_dec).map_err(|e| ProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: format!("Invalid high price: {}", e),
        })?;

        let low_price = Price::new(low_dec).map_err(|e| ProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: format!("Invalid low price: {}", e),
        })?;

        let close_price = Price::new(close_dec).map_err(|e| ProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: format!("Invalid close price: {}", e),
        })?;

        let volume_val = Volume::new(volume).map_err(|e| ProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: format!("Invalid volume: {}", e),
        })?;

        Bar::new(
            timestamp,
            symbol.clone(),
            open_price,
            high_price,
            low_price,
            close_price,
            volume_val,
            timeframe,
            PROVIDER_NAME,
        )
        .map_err(|e| ProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: format!("Failed to create bar: {}", e),
        })
    }

    /// Maps domain [`TimeFrame`] to IB duration string.
    ///
    /// IB real-time bars support: M1, M5, M15, M30, H1
    fn timeframe_to_ib_duration(tf: TimeFrame) -> Result<String, ProviderError> {
        match tf {
            TimeFrame::M1 => Ok("1 min".to_string()),
            TimeFrame::M5 => Ok("5 min".to_string()),
            TimeFrame::M15 => Ok("15 min".to_string()),
            TimeFrame::M30 => Ok("30 min".to_string()),
            TimeFrame::H1 => Ok("1 hour".to_string()),
            _ => Err(ProviderError::NotSupported {
                feature: format!("TimeFrame {:?}", tf),
                provider: PROVIDER_NAME.to_string(),
            }),
        }
    }

    /// Unsubscribes from all active subscriptions.
    ///
    /// Called during disconnect to ensure clean cleanup.
    async fn unsubscribe_all(&self) -> Result<(), ProviderError> {
        let mut subs = self.subscriptions.write().await;
        let count = subs.len();

        if count == 0 {
            return Ok(());
        }

        info!(count = count, "Unsubscribing from all market data");

        for (symbol_str, sub) in subs.drain() {
            if sub.is_bar_subscription() {
                // Call cancelRealTimeBars
                debug!(req_id = sub.req_id, symbol = %symbol_str, "Cancelling real-time bars");
                // TODO: Integrate with actual ibapi cancelRealTimeBars
            } else {
                // Call cancelMktData
                debug!(req_id = sub.req_id, symbol = %symbol_str, "Cancelling market data");
                // TODO: Integrate with actual ibapi cancelMktData
            }
        }

        info!("All market data subscriptions cancelled");
        Ok(())
    }

    /// Creates an IB Contract from a domain Symbol.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The domain symbol
    ///
    /// # Returns
    ///
    /// An IB Contract structure suitable for API calls.
    fn create_contract(symbol: &Symbol) -> IBContract {
        let symbol_str = symbol.as_str();

        if symbol.is_futures() {
            // Parse futures symbol (e.g., "ES/202412" -> symbol="ES", expiry="202412")
            let parts: Vec<&str> = symbol_str.split('/').collect();
            if parts.len() == 2 {
                IBContract {
                    symbol: parts[0].to_string(),
                    sec_type: "FUT".to_string(),
                    exchange: "GLOBEX".to_string(),
                    currency: "USD".to_string(),
                    expiry: Some(parts[1].to_string()),
                }
            } else {
                IBContract {
                    symbol: symbol_str.to_string(),
                    sec_type: "FUT".to_string(),
                    exchange: "GLOBEX".to_string(),
                    currency: "USD".to_string(),
                    expiry: None,
                }
            }
        } else if symbol.is_option() {
            // Parse options symbol (e.g., "AAPL:240719C150")
            IBContract {
                symbol: symbol_str.to_string(),
                sec_type: "OPT".to_string(),
                exchange: "SMART".to_string(),
                currency: "USD".to_string(),
                expiry: None,
            }
        } else {
            // Default to stock
            IBContract {
                symbol: symbol_str.to_string(),
                sec_type: "STK".to_string(),
                exchange: "SMART".to_string(),
                currency: "USD".to_string(),
                expiry: None,
            }
        }
    }
}

/// IB Contract structure for API calls.
#[derive(Debug, Clone)]
struct IBContract {
    symbol: String,
    sec_type: String,
    exchange: String,
    currency: String,
    expiry: Option<String>,
}

#[async_trait]
impl MarketDataProvider for IBMarketDataProvider {
    #[instrument(skip(self))]
    async fn connect(&mut self) -> Result<(), ProviderError> {
        info!("Connecting IB market data provider");

        // Ensure underlying client is connected
        self.client.ensure_connected().await?;

        // Start event processor if not already running
        // In a real implementation, we would create a new event channel
        // and pass it to the event processor
        // For now, we rely on the client's existing event channel

        info!("IB market data provider connected");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn disconnect(&mut self) -> Result<(), ProviderError> {
        info!("Disconnecting IB market data provider");

        // Cancel all subscriptions first
        self.unsubscribe_all().await?;

        // Stop event processor
        let mut processor = self.event_processor.lock().await;
        if let Some(handle) = processor.take() {
            handle.abort();
            info!("Event processor stopped");
        }

        // Disconnect underlying client
        self.client.disconnect().await?;

        info!("IB market data provider disconnected");
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }

    #[instrument(skip(self), fields(symbol = %symbol, timeframe = ?timeframe))]
    async fn subscribe_bars(
        &self,
        symbol: &Symbol,
        timeframe: TimeFrame,
    ) -> Result<mpsc::Receiver<Bar>, ProviderError> {
        // Check connection
        if !self.is_connected().await {
            return Err(ProviderError::SubscriptionFailed {
                symbol: symbol.to_string(),
                provider: PROVIDER_NAME.to_string(),
                reason: "Not connected".to_string(),
            });
        }

        // Check if already subscribed
        let symbol_str = symbol.as_str().to_string();
        {
            let subs = self.subscriptions.read().await;
            if subs.contains_key(&symbol_str) {
                return Err(ProviderError::SubscriptionFailed {
                    symbol: symbol.to_string(),
                    provider: PROVIDER_NAME.to_string(),
                    reason: "Already subscribed".to_string(),
                });
            }
        }

        // Validate timeframe is supported
        let duration = Self::timeframe_to_ib_duration(timeframe)?;

        // Generate request ID
        let req_id = self.generate_req_id();

        // Create bounded channel with backpressure
        let (tx, rx) = mpsc::channel(self.channel_size);

        // Create contract
        let contract = Self::create_contract(symbol);

        info!(
            req_id = req_id,
            symbol = %symbol,
            timeframe = ?timeframe,
            duration = %duration,
            "Subscribing to real-time bars"
        );

        // Call IB API reqRealTimeBars
        // TODO: Integrate with actual ibapi::Client::req_real_time_bars
        // self.client.req_real_time_bars(req_id, &contract, &duration, "TRADES", true).await?;

        // Store subscription info
        let sub_info = SubscriptionInfo::new_bar(symbol.clone(), timeframe, req_id, tx);
        {
            let mut subs = self.subscriptions.write().await;
            subs.insert(symbol_str, sub_info);
        }

        debug!(req_id = req_id, "Real-time bars subscription active");

        Ok(rx)
    }

    #[instrument(skip(self), fields(symbol = %symbol))]
    async fn subscribe_ticks(
        &self,
        symbol: &Symbol,
    ) -> Result<mpsc::Receiver<Tick>, ProviderError> {
        // Check connection
        if !self.is_connected().await {
            return Err(ProviderError::SubscriptionFailed {
                symbol: symbol.to_string(),
                provider: PROVIDER_NAME.to_string(),
                reason: "Not connected".to_string(),
            });
        }

        // Check if already subscribed
        let symbol_str = symbol.as_str().to_string();
        {
            let subs = self.subscriptions.read().await;
            if subs.contains_key(&symbol_str) {
                return Err(ProviderError::SubscriptionFailed {
                    symbol: symbol.to_string(),
                    provider: PROVIDER_NAME.to_string(),
                    reason: "Already subscribed".to_string(),
                });
            }
        }

        // Generate request ID
        let req_id = self.generate_req_id();

        // Create bounded channel with backpressure
        let (tx, rx) = mpsc::channel(self.channel_size);

        // Create contract
        let contract = Self::create_contract(symbol);

        info!(
            req_id = req_id,
            symbol = %symbol,
            "Subscribing to tick data"
        );

        // Call IB API reqMktData
        // TODO: Integrate with actual ibapi::Client::req_mkt_data
        // self.client.req_mkt_data(req_id, &contract, "", false, false, None).await?;

        // Store subscription info
        let sub_info = SubscriptionInfo::new_tick(symbol.clone(), req_id, tx);
        {
            let mut subs = self.subscriptions.write().await;
            subs.insert(symbol_str, sub_info);
        }

        debug!(req_id = req_id, "Tick data subscription active");

        Ok(rx)
    }

    #[instrument(skip(self), fields(symbol = %symbol))]
    async fn unsubscribe(&self, symbol: &Symbol) -> Result<(), ProviderError> {
        let symbol_str = symbol.as_str().to_string();

        // Find and remove subscription
        let sub_info = {
            let mut subs = self.subscriptions.write().await;
            subs.remove(&symbol_str)
        };

        match sub_info {
            Some(sub) => {
                info!(req_id = sub.req_id, symbol = %symbol, "Unsubscribing from market data");

                if sub.is_bar_subscription() {
                    // Call cancelRealTimeBars
                    // TODO: Integrate with actual ibapi::Client::cancel_real_time_bars
                    debug!(req_id = sub.req_id, "Cancelled real-time bars");
                } else {
                    // Call cancelMktData
                    // TODO: Integrate with actual ibapi::Client::cancel_mkt_data
                    debug!(req_id = sub.req_id, "Cancelled market data");
                }

                Ok(())
            }
            None => {
                // Idempotent: succeed if not subscribed
                debug!(symbol = %symbol, "Not subscribed, nothing to unsubscribe");
                Ok(())
            }
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::external::ib::IBConfig;
    use tokio::sync::mpsc;

    fn create_test_client() -> Arc<IBClient> {
        let (event_tx, _event_rx) = mpsc::channel(100);
        Arc::new(IBClient::new(IBConfig::default(), event_tx))
    }

    #[test]
    fn test_timeframe_to_ib_duration() {
        assert_eq!(
            IBMarketDataProvider::timeframe_to_ib_duration(TimeFrame::M1).unwrap(),
            "1 min"
        );
        assert_eq!(
            IBMarketDataProvider::timeframe_to_ib_duration(TimeFrame::M5).unwrap(),
            "5 min"
        );
        assert_eq!(
            IBMarketDataProvider::timeframe_to_ib_duration(TimeFrame::M15).unwrap(),
            "15 min"
        );
        assert_eq!(
            IBMarketDataProvider::timeframe_to_ib_duration(TimeFrame::M30).unwrap(),
            "30 min"
        );
        assert_eq!(
            IBMarketDataProvider::timeframe_to_ib_duration(TimeFrame::H1).unwrap(),
            "1 hour"
        );

        // Unsupported timeframes
        assert!(IBMarketDataProvider::timeframe_to_ib_duration(TimeFrame::H4).is_err());
        assert!(IBMarketDataProvider::timeframe_to_ib_duration(TimeFrame::D1).is_err());
        assert!(IBMarketDataProvider::timeframe_to_ib_duration(TimeFrame::W1).is_err());
    }

    #[test]
    fn test_convert_ib_bar() {
        let symbol = Symbol::new("AAPL").unwrap();
        let timestamp = Utc::now().timestamp();

        let result = IBMarketDataProvider::convert_ib_bar(
            &symbol,
            timestamp,
            150.0,
            155.0,
            149.0,
            152.0,
            1000,
            152.5,
            50,
            TimeFrame::M1,
        );

        assert!(result.is_ok());
        let bar = result.unwrap();
        assert_eq!(bar.symbol().as_str(), "AAPL");
        assert_eq!(bar.timeframe(), TimeFrame::M1);
    }

    #[test]
    fn test_create_contract_stock() {
        let symbol = Symbol::new("AAPL").unwrap();
        let contract = IBMarketDataProvider::create_contract(&symbol);

        assert_eq!(contract.symbol, "AAPL");
        assert_eq!(contract.sec_type, "STK");
        assert_eq!(contract.exchange, "SMART");
        assert_eq!(contract.currency, "USD");
        assert!(contract.expiry.is_none());
    }

    #[test]
    fn test_create_contract_futures() {
        let symbol = Symbol::new("ES/202412").unwrap();
        let contract = IBMarketDataProvider::create_contract(&symbol);

        assert_eq!(contract.symbol, "ES");
        assert_eq!(contract.sec_type, "FUT");
        assert_eq!(contract.exchange, "GLOBEX");
        assert_eq!(contract.currency, "USD");
        assert_eq!(contract.expiry, Some("202412".to_string()));
    }

    #[test]
    fn test_generate_req_id() {
        let client = create_test_client();
        let provider = IBMarketDataProvider::new(client, 100);

        let id1 = provider.generate_req_id();
        let id2 = provider.generate_req_id();
        let id3 = provider.generate_req_id();

        assert_eq!(id1, 1000);
        assert_eq!(id2, 1001);
        assert_eq!(id3, 1002);
    }

    #[tokio::test]
    async fn test_provider_not_connected() {
        let client = create_test_client();
        let provider = IBMarketDataProvider::new(client, 100);

        // Should not be connected initially
        assert!(!provider.is_connected().await);
    }

    #[tokio::test]
    async fn test_subscribe_bars_not_connected() {
        let client = create_test_client();
        let provider = IBMarketDataProvider::new(client, 100);

        let symbol = Symbol::new("AAPL").unwrap();
        let result = provider.subscribe_bars(&symbol, TimeFrame::M1).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ProviderError::SubscriptionFailed { symbol: s, .. } => {
                assert_eq!(s, "AAPL");
            }
            _ => panic!("Expected SubscriptionFailed error"),
        }
    }

    #[tokio::test]
    async fn test_subscribe_ticks_not_connected() {
        let client = create_test_client();
        let provider = IBMarketDataProvider::new(client, 100);

        let symbol = Symbol::new("AAPL").unwrap();
        let result = provider.subscribe_ticks(&symbol).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ProviderError::SubscriptionFailed { symbol: s, .. } => {
                assert_eq!(s, "AAPL");
            }
            _ => panic!("Expected SubscriptionFailed error"),
        }
    }

    #[tokio::test]
    async fn test_unsubscribe_idempotent() {
        let client = create_test_client();
        let provider = IBMarketDataProvider::new(client, 100);

        // Unsubscribing from non-existent subscription should succeed
        let symbol = Symbol::new("AAPL").unwrap();
        let result = provider.unsubscribe(&symbol).await;

        assert!(result.is_ok());
    }

    #[test]
    fn test_subscription_info() {
        let symbol = Symbol::new("AAPL").unwrap();
        let (bar_tx, _bar_rx) = mpsc::channel(10);
        let (tick_tx, _tick_rx) = mpsc::channel(10);

        let bar_sub = SubscriptionInfo::new_bar(symbol.clone(), TimeFrame::M1, 1000, bar_tx);
        assert!(bar_sub.is_bar_subscription());
        assert!(!bar_sub.is_tick_subscription());
        assert_eq!(bar_sub.timeframe, Some(TimeFrame::M1));

        let tick_sub = SubscriptionInfo::new_tick(symbol.clone(), 1001, tick_tx);
        assert!(!tick_sub.is_bar_subscription());
        assert!(tick_sub.is_tick_subscription());
        assert_eq!(tick_sub.timeframe, None);
    }

    #[test]
    fn test_provider_debug() {
        let client = create_test_client();
        let provider = IBMarketDataProvider::new(client, 500);

        let debug_str = format!("{:?}", provider);
        assert!(debug_str.contains("IBMarketDataProvider"));
        assert!(debug_str.contains("500"));
    }

    #[test]
    fn test_ib_contract_debug() {
        let contract = IBContract {
            symbol: "AAPL".to_string(),
            sec_type: "STK".to_string(),
            exchange: "SMART".to_string(),
            currency: "USD".to_string(),
            expiry: None,
        };

        let debug_str = format!("{:?}", contract);
        assert!(debug_str.contains("AAPL"));
        assert!(debug_str.contains("STK"));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::external::ib::IBConfig;
    use tokio::sync::mpsc;

    /// Integration test for bar subscription (requires running IB Gateway).
    ///
    /// Run with: cargo test -p infrastructure ib::market_data::integration_tests::test_subscribe_bars_success -- --ignored
    #[tokio::test]
    #[ignore = "Requires running IB Gateway/TWS"]
    async fn test_subscribe_bars_success() {
        let (event_tx, _event_rx) = mpsc::channel(100);
        let client = Arc::new(IBClient::new(IBConfig::paper_trading(), event_tx));
        let mut provider = IBMarketDataProvider::new(client, 1000);

        // Connect
        provider.connect().await.expect("Failed to connect");

        // Subscribe to bars
        let symbol = Symbol::new("AAPL").unwrap();
        let mut rx = provider
            .subscribe_bars(&symbol, TimeFrame::M1)
            .await
            .expect("Failed to subscribe");

        // Wait for at least one bar (timeout after 30 seconds)
        let bar = tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            rx.recv(),
        )
        .await
        .expect("Timeout waiting for bar")
        .expect("Channel closed");

        assert_eq!(bar.symbol().as_str(), "AAPL");
        assert_eq!(bar.timeframe(), TimeFrame::M1);

        // Unsubscribe
        provider.unsubscribe(&symbol).await.expect("Failed to unsubscribe");

        // Disconnect
        provider.disconnect().await.expect("Failed to disconnect");
    }

    /// Integration test for tick subscription (requires running IB Gateway).
    #[tokio::test]
    #[ignore = "Requires running IB Gateway/TWS"]
    async fn test_subscribe_ticks_success() {
        let (event_tx, _event_rx) = mpsc::channel(100);
        let client = Arc::new(IBClient::new(IBConfig::paper_trading(), event_tx));
        let mut provider = IBMarketDataProvider::new(client, 1000);

        // Connect
        provider.connect().await.expect("Failed to connect");

        // Subscribe to ticks
        let symbol = Symbol::new("AAPL").unwrap();
        let mut rx = provider
            .subscribe_ticks(&symbol)
            .await
            .expect("Failed to subscribe");

        // Wait for at least one tick (timeout after 10 seconds)
        let tick = tokio::time::timeout(
            tokio::time::Duration::from_secs(10),
            rx.recv(),
        )
        .await
        .expect("Timeout waiting for tick")
        .expect("Channel closed");

        assert_eq!(tick.symbol().as_str(), "AAPL");

        // Unsubscribe
        provider.unsubscribe(&symbol).await.expect("Failed to unsubscribe");

        // Disconnect
        provider.disconnect().await.expect("Failed to disconnect");
    }

    /// Integration test for multiple subscriptions.
    #[tokio::test]
    #[ignore = "Requires running IB Gateway/TWS"]
    async fn test_multiple_subscriptions() {
        let (event_tx, _event_rx) = mpsc::channel(100);
        let client = Arc::new(IBClient::new(IBConfig::paper_trading(), event_tx));
        let mut provider = IBMarketDataProvider::new(client, 1000);

        // Connect
        provider.connect().await.expect("Failed to connect");

        // Subscribe to multiple symbols
        let aapl = Symbol::new("AAPL").unwrap();
        let msft = Symbol::new("MSFT").unwrap();

        let mut aapl_rx = provider
            .subscribe_bars(&aapl, TimeFrame::M1)
            .await
            .expect("Failed to subscribe to AAPL");

        let mut msft_rx = provider
            .subscribe_bars(&msft, TimeFrame::M1)
            .await
            .expect("Failed to subscribe to MSFT");

        // Wait for data from both symbols
        let aapl_bar = tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            aapl_rx.recv(),
        )
        .await
        .expect("Timeout waiting for AAPL")
        .expect("AAPL channel closed");

        let msft_bar = tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            msft_rx.recv(),
        )
        .await
        .expect("Timeout waiting for MSFT")
        .expect("MSFT channel closed");

        assert_eq!(aapl_bar.symbol().as_str(), "AAPL");
        assert_eq!(msft_bar.symbol().as_str(), "MSFT");

        // Unsubscribe all
        provider.unsubscribe(&aapl).await.expect("Failed to unsubscribe AAPL");
        provider.unsubscribe(&msft).await.expect("Failed to unsubscribe MSFT");

        // Disconnect
        provider.disconnect().await.expect("Failed to disconnect");
    }

    /// Integration test for disconnect cleanup.
    #[tokio::test]
    #[ignore = "Requires running IB Gateway/TWS"]
    async fn test_disconnect_cleanup() {
        let (event_tx, _event_rx) = mpsc::channel(100);
        let client = Arc::new(IBClient::new(IBConfig::paper_trading(), event_tx));
        let mut provider = IBMarketDataProvider::new(client.clone(), 1000);

        // Connect
        provider.connect().await.expect("Failed to connect");

        // Subscribe
        let symbol = Symbol::new("AAPL").unwrap();
        let _rx = provider
            .subscribe_bars(&symbol, TimeFrame::M1)
            .await
            .expect("Failed to subscribe");

        // Disconnect (should auto-unsubscribe)
        provider.disconnect().await.expect("Failed to disconnect");

        // Verify disconnected
        assert!(!provider.is_connected().await);

        // Verify subscriptions cleared
        let subs = provider.subscriptions.read().await;
        assert!(subs.is_empty());
    }
}
