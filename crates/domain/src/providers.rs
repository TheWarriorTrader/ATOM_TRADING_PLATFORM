//! # Provider Traits
//!
//! Abstract provider interfaces for external data and execution services.
//! These traits define the contract that infrastructure implementations must fulfill
//! for connecting to market data feeds and brokerage services.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::entities::{Bar, Tick};
use crate::errors::ProviderError;
use crate::values::{Symbol, TimeFrame};

/// Market data provider trait.
///
/// This trait defines the contract for connecting to and consuming real-time
/// market data from external providers (e.g., Interactive Brokers, Polygon.io,
/// Alpaca, etc.).
///
/// # Architecture
///
/// The provider pattern separates connection management from data consumption:
/// 1. Connect to the provider using [`connect`](Self::connect)
/// 2. Subscribe to symbols using [`subscribe_bars`](Self::subscribe_bars) or [`subscribe_ticks`](Self::subscribe_ticks)
/// 3. Receive data through the returned mpsc channels
/// 4. Unsubscribe when done using [`unsubscribe`](Self::unsubscribe)
/// 5. Disconnect using [`disconnect`](Self::disconnect)
///
/// # Concurrency
///
/// - The provider must be thread-safe (`Send + Sync`)
/// - Multiple subscriptions can be active simultaneously
/// - Each subscription returns a separate channel for isolation
///
/// # Error Handling
///
/// All operations return [`ProviderError`] which includes specific variants for:
/// - Connection failures
/// - Authentication errors
/// - Subscription failures
/// - Rate limiting
/// - Invalid responses
///
/// # Examples
///
/// ```rust,ignore
/// use domain::providers::MarketDataProvider;
/// use domain::values::{Symbol, TimeFrame};
///
/// async fn consume_bars<P: MarketDataProvider>(
///     provider: &mut P,
/// ) -> Result<(), Box<dyn std::error::Error>> {
///     // Connect to the provider
///     provider.connect().await?;
///
///     // Subscribe to AAPL 1-minute bars
///     let symbol = Symbol::new("AAPL")?;
///     let mut receiver = provider.subscribe_bars(&symbol, TimeFrame::M1).await?;
///
///     // Consume bars
///     while let Some(bar) = receiver.recv().await {
///         println!("Received bar: {}", bar);
///     }
///
///     // Cleanup
///     provider.unsubscribe(&symbol).await?;
///     provider.disconnect().await?;
///
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    /// Establishes connection to the market data provider.
    ///
    /// This method should:
    /// - Authenticate with the provider if required
    /// - Establish network connections
    /// - Initialize any required resources
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::ConnectionFailed`] if the connection cannot be established.
    /// Returns [`ProviderError::AuthenticationFailed`] if authentication fails.
    ///
    /// # Idempotency
    ///
    /// If already connected, this method should succeed without error.
    async fn connect(&mut self) -> Result<(), ProviderError>;

    /// Closes connection to the market data provider.
    ///
    /// This method should:
    /// - Close all active subscriptions
    /// - Release network connections
    /// - Clean up resources
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::ConnectionFailed`] if the disconnect operation fails.
    ///
    /// # Idempotency
    ///
    /// If already disconnected, this method should succeed without error.
    async fn disconnect(&mut self) -> Result<(), ProviderError>;

    /// Checks if the provider is currently connected.
    ///
    /// # Returns
    ///
    /// `true` if connected and ready to accept subscriptions, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::providers::MarketDataProvider;
    ///
    /// async fn check_connection<P: MarketDataProvider>(provider: &P) {
    ///     if provider.is_connected().await {
    ///         println!("Provider is connected");
    ///     } else {
    ///         println!("Provider is disconnected");
    ///     }
    /// }
    /// ```
    async fn is_connected(&self) -> bool;

    /// Subscribes to real-time bar (OHLCV) data for a symbol.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol to subscribe to
    /// * `timeframe` - The bar timeframe (e.g., M1, M5, H1)
    ///
    /// # Returns
    ///
    /// A [`mpsc::Receiver<Bar>`] that receives bars as they are published by the provider.
    /// The channel is unbounded by default; implementations may choose to use bounded channels
    /// with appropriate backpressure strategies.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::SubscriptionFailed`] if:
    /// - The symbol is invalid or not available
    /// - The timeframe is not supported
    /// - Not connected to the provider
    /// - Subscription limit reached
    ///
    /// Returns [`ProviderError::NotSupported`] if real-time bars are not supported by this provider.
    ///
    /// # Concurrency
    ///
    /// Multiple subscriptions for different symbols/timeframes can be active simultaneously.
    /// Each subscription returns a separate channel.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::providers::MarketDataProvider;
    /// use domain::values::{Symbol, TimeFrame};
    ///
    /// async fn subscribe_minute_bars<P: MarketDataProvider>(
    ///     provider: &P,
    /// ) -> Result<tokio::sync::mpsc::Receiver<domain::entities::Bar>, ProviderError> {
    ///     let symbol = Symbol::new("AAPL").unwrap();
    ///     provider.subscribe_bars(&symbol, TimeFrame::M1).await
    /// }
    /// ```
    async fn subscribe_bars(
        &self,
        symbol: &Symbol,
        timeframe: TimeFrame,
    ) -> Result<mpsc::Receiver<Bar>, ProviderError>;

    /// Subscribes to real-time tick data for a symbol.
    ///
    /// Tick data represents individual trades or quote updates and provides
    /// the highest granularity of market data available.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol to subscribe to
    ///
    /// # Returns
    ///
    /// A [`mpsc::Receiver<Tick>`] that receives ticks as they are published by the provider.
    /// The channel is unbounded by default; implementations may choose to use bounded channels
    /// with appropriate backpressure strategies.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::SubscriptionFailed`] if:
    /// - The symbol is invalid or not available
    /// - Not connected to the provider
    /// - Subscription limit reached
    ///
    /// Returns [`ProviderError::NotSupported`] if tick data is not supported by this provider.
    ///
    /// # Performance Considerations
    ///
    /// Tick data can be extremely high-frequency. Consider:
    /// - Using bounded channels with appropriate capacity
    /// - Implementing backpressure or sampling strategies
    /// - Processing ticks in batches where possible
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::providers::MarketDataProvider;
    /// use domain::values::Symbol;
    ///
    /// async fn subscribe_ticks<P: MarketDataProvider>(
    ///     provider: &P,
    /// ) -> Result<tokio::sync::mpsc::Receiver<domain::entities::Tick>, ProviderError> {
    ///     let symbol = Symbol::new("AAPL").unwrap();
    ///     provider.subscribe_ticks(&symbol).await
    /// }
    /// ```
    async fn subscribe_ticks(&self, symbol: &Symbol) -> Result<mpsc::Receiver<Tick>, ProviderError>;

    /// Unsubscribes from all data streams for a symbol.
    ///
    /// This method should close all active subscriptions (bars and ticks) for the given symbol.
    /// The associated channels will be closed after this call.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol to unsubscribe from
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::SubscriptionFailed`] if:
    /// - Not connected to the provider
    /// - The symbol was not previously subscribed
    ///
    /// # Idempotency
    ///
    /// If the symbol was not subscribed, this method should succeed without error.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use domain::providers::MarketDataProvider;
    /// use domain::values::Symbol;
    ///
    /// async fn unsubscribe_symbol<P: MarketDataProvider>(
    ///     provider: &P,
    /// ) -> Result<(), ProviderError> {
    ///     let symbol = Symbol::new("AAPL").unwrap();
    ///     provider.unsubscribe(&symbol).await
    /// }
    /// ```
    async fn unsubscribe(&self, symbol: &Symbol) -> Result<(), ProviderError>;
}

// =============================================================================
// OBJECT-SAFE WRAPPER
// =============================================================================

/// Object-safe wrapper for [`MarketDataProvider`].
///
/// This trait allows storing `MarketDataProvider` implementations in
/// collections or using them as trait objects (`dyn MarketDataProvider`).
///
/// # Usage
///
/// ```rust,ignore
/// use domain::providers::{MarketDataProvider, MarketDataProviderExt};
///
/// fn store_provider(provider: impl MarketDataProvider) -> Box<dyn MarketDataProvider> {
///     Box::new(provider)
/// }
/// ```
pub trait MarketDataProviderExt: MarketDataProvider {}

impl<T: MarketDataProvider> MarketDataProviderExt for T {}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{Bar, Tick};
    use crate::values::{Price, Side, Symbol, TimeFrame, Volume};
    use chrono::Utc;
    use rust_decimal::Decimal;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// Mock provider for testing purposes
    struct MockProvider {
        connected: Arc<AtomicBool>,
    }

    impl MockProvider {
        fn new() -> Self {
            Self {
                connected: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    #[async_trait]
    impl MarketDataProvider for MockProvider {
        async fn connect(&mut self) -> Result<(), ProviderError> {
            self.connected.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn disconnect(&mut self) -> Result<(), ProviderError> {
            self.connected.store(false, Ordering::SeqCst);
            Ok(())
        }

        async fn is_connected(&self) -> bool {
            self.connected.load(Ordering::SeqCst)
        }

        async fn subscribe_bars(
            &self,
            _symbol: &Symbol,
            _timeframe: TimeFrame,
        ) -> Result<mpsc::Receiver<Bar>, ProviderError> {
            if !self.is_connected().await {
                return Err(ProviderError::SubscriptionFailed {
                    symbol: "TEST".to_string(),
                    provider: "Mock".to_string(),
                    reason: "Not connected".to_string(),
                });
            }
            let (tx, rx) = mpsc::channel(100);
            // Spawn a task that sends a test bar
            let tx = Arc::new(tokio::sync::Mutex::new(tx));
            tokio::spawn(async move {
                let _ = tx.lock().await.send(create_test_bar()).await;
            });
            Ok(rx)
        }

        async fn subscribe_ticks(
            &self,
            _symbol: &Symbol,
        ) -> Result<mpsc::Receiver<Tick>, ProviderError> {
            if !self.is_connected().await {
                return Err(ProviderError::SubscriptionFailed {
                    symbol: "TEST".to_string(),
                    provider: "Mock".to_string(),
                    reason: "Not connected".to_string(),
                });
            }
            let (tx, rx) = mpsc::channel(100);
            let tx = Arc::new(tokio::sync::Mutex::new(tx));
            tokio::spawn(async move {
                let _ = tx.lock().await.send(create_test_tick()).await;
            });
            Ok(rx)
        }

        async fn unsubscribe(&self, _symbol: &Symbol) -> Result<(), ProviderError> {
            Ok(())
        }
    }

    fn create_test_bar() -> Bar {
        Bar::new(
            Utc::now(),
            Symbol::new("TEST").unwrap(),
            Price::new(Decimal::new(10000, 2)).unwrap(),
            Price::new(Decimal::new(10100, 2)).unwrap(),
            Price::new(Decimal::new(9900, 2)).unwrap(),
            Price::new(Decimal::new(10050, 2)).unwrap(),
            Volume::new(1000).unwrap(),
            TimeFrame::M1,
            "MockProvider",
        )
        .unwrap()
    }

    fn create_test_tick() -> Tick {
        Tick::new(
            Utc::now(),
            Symbol::new("TEST").unwrap(),
            Price::new(Decimal::new(10050, 2)).unwrap(),
            Volume::new(100).unwrap(),
            Some(Side::Buy),
            "MockProvider",
        )
        .unwrap()
    }

    #[tokio::test]
    async fn test_mock_provider_connect_disconnect() {
        let mut provider = MockProvider::new();

        assert!(!provider.is_connected().await);

        provider.connect().await.expect("Connect should succeed");
        assert!(provider.is_connected().await);

        provider.disconnect().await.expect("Disconnect should succeed");
        assert!(!provider.is_connected().await);
    }

    #[tokio::test]
    async fn test_mock_provider_subscribe_bars_fails_when_disconnected() {
        let provider = MockProvider::new();
        let symbol = Symbol::new("TEST").unwrap();

        let result = provider.subscribe_bars(&symbol, TimeFrame::M1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_provider_subscribe_ticks_fails_when_disconnected() {
        let provider = MockProvider::new();
        let symbol = Symbol::new("TEST").unwrap();

        let result = provider.subscribe_ticks(&symbol).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_provider_unsubscribe() {
        let mut provider = MockProvider::new();
        provider.connect().await.expect("Connect should succeed");

        let symbol = Symbol::new("TEST").unwrap();
        let result = provider.unsubscribe(&symbol).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_provider_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MockProvider>();
    }
}
