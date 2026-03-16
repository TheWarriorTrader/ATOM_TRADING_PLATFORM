//! # Interactive Brokers Module
//!
//! Production-ready integration with Interactive Brokers TWS/Gateway.
//!
//! ## Overview
//!
//! This module provides a safe, async wrapper around the Interactive Brokers API
//! with the following features:
//!
//! - **Connection Management**: Automatic connection with retry logic
//! - **Auto-reconnection**: Exponential backoff with circuit breaker
//! - **Heartbeat Monitoring**: Periodic health checks with dead connection detection
//! - **Event-driven Architecture**: MPSC channel for async event handling
//! - **Type Safety**: Strong typing with `Symbol`, `Price`, `Quantity` from domain
//!
//! ## Quick Start
//!
//! ```rust
//! use infrastructure::external::ib::{IBClient, IBConfig, IBEvent};
//! use tokio::sync::mpsc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create event channel
//! let (event_tx, mut event_rx) = mpsc::channel(100);
//!
//! // Configure client (paper trading by default)
//! let config = IBConfig::default();
//!
//! // Create client
//! let client = IBClient::new(config, event_tx);
//!
//! // Connect to IB Gateway/TWS
//! client.connect().await?;
//!
//! // Handle events
//! tokio::spawn(async move {
//!     while let Some(event) = event_rx.recv().await {
//!         match event {
//!             IBEvent::Connected { .. } => println!("Connected!"),
//!             IBEvent::MarketData { req_id, data } => {
//!                 println!("Market data for req {}: {:?}", req_id, data);
//!             }
//!             _ => {}
//!         }
//!     }
//! });
//!
//! // Get next valid order ID before placing orders
//! let order_id = client.request_next_order_id().await?;
//!
//! # Ok(())
//! # }
//! ```
//!
//! ## Configuration
//!
//! Use environment variables or create `IBConfig` programmatically:
//!
//! ```rust
//! use infrastructure::external::ib::IBConfig;
//!
//! // Paper trading (default, port 7497)
//! let paper_config = IBConfig::paper_trading();
//!
//! // Live trading (port 7496)
//! let live_config = IBConfig::live_trading();
//!
//! // Custom configuration
//! let custom = IBConfig {
//!     host: "192.168.1.100".to_string(),
//!     port: 7497,
//!     client_id: 42,
//!     ..Default::default()
//! };
//! ```

pub use client::{
    map_ib_error, ConnectionState, IBClient, IBConfig, IBEvent, MarketDataUpdate,
};
pub use market_data::IBMarketDataProvider;
pub use execution::{IBExecutionGateway, IBOrder, IBContract, IBExecution};

pub mod client;
pub mod market_data;
pub mod execution;

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    /// Integration test placeholder for connection to real IB Gateway.
    ///
    /// This test is ignored by default as it requires a running IB Gateway
    /// or TWS instance. To run:
    ///
    /// ```bash
    /// cargo test -p infrastructure ib::tests::integration_connection -- --ignored
    /// ```
    #[tokio::test]
    #[ignore = "Requires running IB Gateway/TWS"]
    async fn integration_connection() {
        let (event_tx, mut event_rx) = mpsc::channel(100);
        let config = IBConfig::paper_trading();
        let client = IBClient::new(config, event_tx);

        // Attempt connection
        client.connect().await.expect("Failed to connect to IB");

        assert!(client.is_connected().await);

        // Wait for connected event
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            event_rx.recv(),
        )
        .await
        .expect("Timeout waiting for event")
        .expect("Event channel closed");

        assert!(
            matches!(event, IBEvent::Connected { .. }),
            "Expected Connected event, got {:?}",
            event
        );

        // Disconnect
        client.disconnect().await.expect("Failed to disconnect");
    }

    /// Integration test for order ID retrieval.
    #[tokio::test]
    #[ignore = "Requires running IB Gateway/TWS"]
    async fn integration_next_valid_id() {
        let (event_tx, _event_rx) = mpsc::channel(100);
        let config = IBConfig::paper_trading();
        let client = IBClient::new(config, event_tx);

        client.connect().await.expect("Failed to connect");

        let order_id = client
            .request_next_order_id()
            .await
            .expect("Failed to get next valid ID");

        assert!(order_id > 0, "Order ID should be positive");

        client.disconnect().await.expect("Failed to disconnect");
    }
}
