//! # Prometheus Metrics Module
//!
//! Provides Prometheus-compatible metrics collection and exposition for the trading platform.
//!
//! ## Features
//!
//! - **Counter Metrics**: bars_ingested_total, bars_persisted_total
//! - **Gauge Metrics**: processing_rate, provider_connected, circuit_breaker_state
//! - **Histogram Metrics**: db_query_latency_ms
//! - **Registry Management**: Thread-safe singleton pattern
//!
//! ## Example
//!
//! ```rust,no_run
//! use infrastructure::metrics::{MetricsCollector, record_bars_ingested, record_db_latency};
//!
//! // Record metrics
//! record_bars_ingested(100);
//! record_db_latency(25.5);
//!
//! // Get Prometheus exposition format
//! let output = MetricsCollector::global().render();
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use once_cell::sync::Lazy;
use prometheus::{
    register_counter, register_gauge, register_histogram, Counter, Encoder, Gauge, Histogram,
    Registry, TextEncoder,
};
use tracing::{debug, error, warn};

// =============================================================================
// GLOBAL REGISTRY
// =============================================================================

/// Global metrics registry singleton.
static GLOBAL_REGISTRY: Lazy<Arc<MetricsCollector>> = Lazy::new(|| {
    Arc::new(MetricsCollector::new().expect("Failed to initialize metrics collector"))
});

// =============================================================================
// METRICS COLLECTOR
// =============================================================================

/// Thread-safe metrics collector for the trading platform.
///
/// Collects and exposes Prometheus-compatible metrics for monitoring
/// the health and performance of the trading system.
#[derive(Debug)]
pub struct MetricsCollector {
    /// Prometheus registry.
    registry: Registry,

    // Counter metrics
    /// Total number of bars ingested from market data providers.
    bars_ingested_total: Counter,
    /// Total number of bars persisted to the database.
    bars_persisted_total: Counter,
    /// Total number of errors encountered.
    errors_total: Counter,

    // Gauge metrics
    /// Current processing rate in bars per second.
    processing_rate: Gauge,
    /// Connection status: 1 = connected, 0 = disconnected.
    provider_connected: Gauge,
    /// Circuit breaker state: 0 = closed, 1 = open, 2 = half-open.
    circuit_breaker_state: Gauge,
    /// Current buffer depth (number of bars in memory buffer).
    buffer_depth: Gauge,
    /// Number of active subscribers to market data.
    active_subscribers: Gauge,

    // Histogram metrics
    /// Database query latency distribution in milliseconds.
    db_query_latency_ms: Histogram,

    // Internal tracking
    /// Last time processing rate was calculated.
    last_rate_calculation: Mutex<Instant>,
    /// Bars ingested at last rate calculation.
    bars_at_last_calculation: AtomicU64,
}

impl MetricsCollector {
    /// Creates a new metrics collector with all registered metrics.
    ///
    /// # Errors
    ///
    /// Returns an error if metric registration fails (e.g., duplicate registration).
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new_custom(
            Some("trading_platform".to_string()),
            Some(
                vec![("version".to_string(), env!("CARGO_PKG_VERSION").to_string())]
                    .into_iter()
                    .collect(),
            ),
        )?;

        // Counter metrics
        let bars_ingested_total = register_counter!(
            "bars_ingested_total",
            "Total number of bars ingested from market data providers"
        )?;

        let bars_persisted_total = register_counter!(
            "bars_persisted_total",
            "Total number of bars persisted to the database"
        )?;

        let errors_total = register_counter!(
            "errors_total",
            "Total number of errors encountered in the pipeline"
        )?;

        // Gauge metrics
        let processing_rate = register_gauge!(
            "processing_rate",
            "Current processing rate in bars per second"
        )?;

        let provider_connected = register_gauge!(
            "provider_connected",
            "Market data provider connection status: 1 = connected, 0 = disconnected"
        )?;

        let circuit_breaker_state = register_gauge!(
            "circuit_breaker_state",
            "Circuit breaker state: 0 = closed, 1 = open, 2 = half-open"
        )?;

        let buffer_depth = register_gauge!(
            "buffer_depth",
            "Current number of bars in the in-memory buffer"
        )?;

        let active_subscribers = register_gauge!(
            "active_subscribers",
            "Number of active market data subscribers"
        )?;

        // Histogram metrics - latency buckets optimized for DB queries (0.5ms to 10s)
        let db_query_latency_ms = register_histogram!(
            "db_query_latency_ms",
            "Database query latency in milliseconds",
            vec![0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0]
        )?;

        // Register all metrics with the custom registry
        registry.register(Box::new(bars_ingested_total.clone()))?;
        registry.register(Box::new(bars_persisted_total.clone()))?;
        registry.register(Box::new(errors_total.clone()))?;
        registry.register(Box::new(processing_rate.clone()))?;
        registry.register(Box::new(provider_connected.clone()))?;
        registry.register(Box::new(circuit_breaker_state.clone()))?;
        registry.register(Box::new(buffer_depth.clone()))?;
        registry.register(Box::new(active_subscribers.clone()))?;
        registry.register(Box::new(db_query_latency_ms.clone()))?;

        Ok(Self {
            registry,
            bars_ingested_total,
            bars_persisted_total,
            errors_total,
            processing_rate,
            provider_connected,
            circuit_breaker_state,
            buffer_depth,
            active_subscribers,
            db_query_latency_ms,
            last_rate_calculation: Mutex::new(Instant::now()),
            bars_at_last_calculation: AtomicU64::new(0),
        })
    }

    /// Returns the global metrics collector instance.
    #[must_use]
    pub fn global() -> Arc<MetricsCollector> {
        Arc::clone(&GLOBAL_REGISTRY)
    }

    /// Renders metrics in Prometheus exposition format.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails.
    pub fn render(&self) -> Result<String, prometheus::Error> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        String::from_utf8(buffer).map_err(|e| {
            prometheus::Error::Msg(format!("Failed to encode metrics to UTF-8: {e}"))
        })
    }

    // =========================================================================
    // COUNTER OPERATIONS
    // =========================================================================

    /// Records ingested bars.
    ///
    /// # Arguments
    ///
    /// * `count` - Number of bars ingested
    pub fn record_bars_ingested(&self, count: u64) {
        self.bars_ingested_total.inc_by(count as f64);
        debug!(count, "Recorded bars ingested");
    }

    /// Records persisted bars.
    ///
    /// # Arguments
    ///
    /// * `count` - Number of bars persisted to database
    pub fn record_bars_persisted(&self, count: u64) {
        self.bars_persisted_total.inc_by(count as f64);
        debug!(count, "Recorded bars persisted");
    }

    /// Records an error occurrence.
    pub fn record_error(&self) {
        self.errors_total.inc();
        debug!("Recorded error");
    }

    // =========================================================================
    // GAUGE OPERATIONS
    // =========================================================================

    /// Updates the processing rate (bars per second).
    ///
    /// Calculates rate based on difference from last calculation.
    pub fn update_processing_rate(&self) {
        let current_bars = self.bars_ingested_total.get() as u64;
        let last_bars = self
            .bars_at_last_calculation
            .swap(current_bars, Ordering::Relaxed);

        let mut last_calc = self.last_rate_calculation.lock().unwrap_or_else(|poisoned| {
            warn!("Rate calculation mutex poisoned, recovering");
            poisoned.into_inner()
        });

        let elapsed = last_calc.elapsed().as_secs_f64();
        *last_calc = Instant::now();

        if elapsed > 0.0 {
            let rate = (current_bars.saturating_sub(last_bars)) as f64 / elapsed;
            self.processing_rate.set(rate);
            debug!(rate, "Updated processing rate");
        }
    }

    /// Sets the provider connection status.
    ///
    /// # Arguments
    ///
    /// * `connected` - `true` if provider is connected, `false` otherwise
    pub fn set_provider_connected(&self, connected: bool) {
        let value = if connected { 1.0 } else { 0.0 };
        self.provider_connected.set(value);
        debug!(connected, "Updated provider connection status");
    }

    /// Sets the circuit breaker state.
    ///
    /// # Arguments
    ///
    /// * `state` - 0 = closed, 1 = open, 2 = half-open
    pub fn set_circuit_breaker_state(&self, state: u8) {
        self.circuit_breaker_state.set(f64::from(state));
        debug!(state, "Updated circuit breaker state");
    }

    /// Sets the current buffer depth.
    ///
    /// # Arguments
    ///
    /// * `depth` - Number of bars currently in buffer
    pub fn set_buffer_depth(&self, depth: usize) {
        self.buffer_depth.set(depth as f64);
    }

    /// Sets the number of active subscribers.
    ///
    /// # Arguments
    ///
    /// * `count` - Number of active market data subscribers
    pub fn set_active_subscribers(&self, count: usize) {
        self.active_subscribers.set(count as f64);
    }

    // =========================================================================
    // HISTOGRAM OPERATIONS
    // =========================================================================

    /// Records a database query latency observation.
    ///
    /// # Arguments
    ///
    /// * `latency_ms` - Query latency in milliseconds
    pub fn record_db_latency(&self, latency_ms: f64) {
        self.db_query_latency_ms.observe(latency_ms);
        debug!(latency_ms, "Recorded DB query latency");
    }

    // =========================================================================
    // GETTERS FOR CURRENT VALUES
    // =========================================================================

    /// Returns the total number of bars ingested.
    #[must_use]
    pub fn bars_ingested_total(&self) -> u64 {
        self.bars_ingested_total.get() as u64
    }

    /// Returns the total number of bars persisted.
    #[must_use]
    pub fn bars_persisted_total(&self) -> u64 {
        self.bars_persisted_total.get() as u64
    }

    /// Returns the current processing rate.
    #[must_use]
    pub fn processing_rate(&self) -> f64 {
        self.processing_rate.get()
    }

    /// Returns the provider connection status (true if connected).
    #[must_use]
    pub fn is_provider_connected(&self) -> bool {
        self.provider_connected.get() > 0.5
    }

    /// Returns the current circuit breaker state.
    #[must_use]
    pub fn circuit_breaker_state(&self) -> u8 {
        self.circuit_breaker_state.get() as u8
    }
}

// =============================================================================
// CONVENIENCE FUNCTIONS
// =============================================================================

/// Records ingested bars to the global metrics collector.
///
/// # Arguments
///
/// * `count` - Number of bars ingested
pub fn record_bars_ingested(count: u64) {
    GLOBAL_REGISTRY.record_bars_ingested(count);
}

/// Records persisted bars to the global metrics collector.
///
/// # Arguments
///
/// * `count` - Number of bars persisted
pub fn record_bars_persisted(count: u64) {
    GLOBAL_REGISTRY.record_bars_persisted(count);
}

/// Records an error occurrence to the global metrics collector.
pub fn record_error() {
    GLOBAL_REGISTRY.record_error();
}

/// Records database query latency to the global metrics collector.
///
/// # Arguments
///
/// * `latency_ms` - Query latency in milliseconds
pub fn record_db_latency(latency_ms: f64) {
    GLOBAL_REGISTRY.record_db_latency(latency_ms);
}

/// Sets the provider connection status in the global metrics collector.
///
/// # Arguments
///
/// * `connected` - `true` if provider is connected
pub fn set_provider_connected(connected: bool) {
    GLOBAL_REGISTRY.set_provider_connected(connected);
}

/// Sets the circuit breaker state in the global metrics collector.
///
/// # Arguments
///
/// * `state` - 0 = closed, 1 = open, 2 = half-open
pub fn set_circuit_breaker_state(state: u8) {
    GLOBAL_REGISTRY.set_circuit_breaker_state(state);
}

/// Sets the buffer depth in the global metrics collector.
///
/// # Arguments
///
/// * `depth` - Current buffer depth
pub fn set_buffer_depth(depth: usize) {
    GLOBAL_REGISTRY.set_buffer_depth(depth);
}

/// Sets the number of active subscribers in the global metrics collector.
///
/// # Arguments
///
/// * `count` - Number of active subscribers
pub fn set_active_subscribers(count: usize) {
    GLOBAL_REGISTRY.set_active_subscribers(count);
}

/// Updates the processing rate calculation in the global metrics collector.
pub fn update_processing_rate() {
    GLOBAL_REGISTRY.update_processing_rate();
}

/// Renders all metrics in Prometheus exposition format.
///
/// # Errors
///
/// Returns an error if encoding fails.
pub fn render_metrics() -> Result<String, prometheus::Error> {
    GLOBAL_REGISTRY.render()
}

// =============================================================================
// METRICS SERVER
// =============================================================================

/// HTTP server configuration for metrics exposition.
#[derive(Debug, Clone)]
pub struct MetricsServerConfig {
    /// Server bind address.
    pub bind_address: String,
    /// Server port.
    pub port: u16,
}

impl Default for MetricsServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
        }
    }
}

/// Metrics HTTP server using Axum.
///
/// Exposes Prometheus metrics on `/metrics` endpoint.
#[derive(Debug, Clone)]
pub struct MetricsServer {
    config: MetricsServerConfig,
}

impl MetricsServer {
    /// Creates a new metrics server with the given configuration.
    #[must_use]
    pub const fn new(config: MetricsServerConfig) -> Self {
        Self { config }
    }

    /// Starts the metrics HTTP server.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to start or encounters a fatal error.
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use axum::{
            routing::get,
            Router,
        };

        let app = Router::new().route("/metrics", get(metrics_handler));

        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        tracing::info!(address = %addr, "Metrics server starting");

        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// HTTP handler for the `/metrics` endpoint.
async fn metrics_handler() -> Result<axum::response::Response<String>, axum::http::StatusCode> {
    match render_metrics() {
        Ok(metrics) => Ok(axum::response::Response::builder()
            .status(200)
            .header("Content-Type", "text/plain; version=0.0.4")
            .body(metrics)
            .unwrap_or_else(|_| {
                axum::response::Response::new("Internal error".to_string())
            })),
        Err(e) => {
            error!(error = %e, "Failed to render metrics");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_registry() {
        // Test that global registry is accessible
        let collector = MetricsCollector::global();
        
        // Test convenience functions don't panic
        record_bars_ingested(1);
        record_bars_persisted(1);
        record_error();
        record_db_latency(10.0);
        set_provider_connected(true);
        set_circuit_breaker_state(0);
        set_buffer_depth(10);
        set_active_subscribers(2);
        update_processing_rate();
        
        // Test metrics are recorded (values may include previous test runs)
        assert!(collector.bars_ingested_total() >= 1);
        assert!(collector.bars_persisted_total() >= 1);
        
        // Test provider status (we set it to true above)
        assert!(collector.is_provider_connected());
        
        // Test circuit breaker state (we set it to 0 above)
        assert_eq!(collector.circuit_breaker_state(), 0);
        
        // Test setting provider to false
        set_provider_connected(false);
        assert!(!collector.is_provider_connected());
    }

    #[test]
    fn test_gauge_operations() {
        let collector = MetricsCollector::global();

        // Test gauge operations
        collector.set_provider_connected(true);
        assert!(collector.is_provider_connected());

        collector.set_provider_connected(false);
        assert!(!collector.is_provider_connected());

        collector.set_circuit_breaker_state(1);
        assert_eq!(collector.circuit_breaker_state(), 1);
        
        collector.set_circuit_breaker_state(0);

        collector.set_buffer_depth(100);
        collector.set_active_subscribers(5);
    }

    #[test]
    fn test_histogram_operations() {
        let collector = MetricsCollector::global();

        // Test histogram operations don't panic
        collector.record_db_latency(25.5);
        collector.record_db_latency(50.0);
        collector.record_db_latency(100.0);
    }

    #[test]
    fn test_render_metrics() {
        let collector = MetricsCollector::global();
        
        // Record some metrics
        collector.record_bars_ingested(10);

        // Render and verify output
        let output = collector.render();
        assert!(output.is_ok());

        let metrics_text = output.unwrap();
        assert!(metrics_text.contains("trading_platform_bars_ingested_total"));
    }
}
