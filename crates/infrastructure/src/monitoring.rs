//! # Monitoring Module
//!
//! Unified monitoring system combining metrics collection and health checks.
//!
//! ## Features
//!
//! - **Metrics Endpoint**: `/metrics` - Prometheus-compatible metrics
//! - **Health Endpoint**: `/health` - Overall system health
//! - **Readiness Probe**: `/health/ready` - Kubernetes readiness probe
//! - **Liveness Probe**: `/health/live` - Kubernetes liveness probe
//!
//! ## Example
//!
//! ```rust,no_run
//! use infrastructure::monitoring::MonitoringServer;
//! use infrastructure::health::HealthChecker;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let health_checker = Arc::new(HealthChecker::new());
//! let server = MonitoringServer::new(8080, health_checker);
//! server.start().await?;
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde_json::json;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::health::{ComponentHealth, ComponentStatus, HealthChecker, OverallHealth};
use crate::metrics::{render_metrics, update_processing_rate};

// =============================================================================
// MONITORING SERVER
// =============================================================================

/// Configuration for the monitoring server.
#[derive(Debug, Clone)]
pub struct MonitoringConfig {
    /// Server bind address.
    pub bind_address: String,
    /// Server port.
    pub port: u16,
    /// Enable Prometheus metrics endpoint.
    pub enable_metrics: bool,
    /// Enable health check endpoints.
    pub enable_health: bool,
    /// Processing rate update interval in seconds.
    pub rate_update_interval_secs: u64,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
            enable_metrics: true,
            enable_health: true,
            rate_update_interval_secs: 10,
        }
    }
}

/// Unified monitoring server handling both metrics and health endpoints.
///
/// This server exposes:
/// - `GET /metrics` - Prometheus metrics
/// - `GET /health` - Overall health status
/// - `GET /health/ready` - Readiness probe
/// - `GET /health/live` - Liveness probe
#[derive(Debug, Clone)]
pub struct MonitoringServer {
    config: MonitoringConfig,
    health_checker: Arc<HealthChecker>,
}

impl MonitoringServer {
    /// Creates a new monitoring server with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration
    /// * `health_checker` - Health checker instance
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use infrastructure::monitoring::{MonitoringServer, MonitoringConfig};
    /// use infrastructure::health::HealthChecker;
    /// use std::sync::Arc;
    ///
    /// let health_checker = Arc::new(HealthChecker::new());
    /// let config = MonitoringConfig::default();
    /// let server = MonitoringServer::new(config, health_checker);
    /// ```
    #[must_use]
    pub fn new(config: MonitoringConfig, health_checker: Arc<HealthChecker>) -> Self {
        Self {
            config,
            health_checker,
        }
    }

    /// Creates a new monitoring server with default configuration on the specified port.
    ///
    /// # Arguments
    ///
    /// * `port` - Server port
    /// * `health_checker` - Health checker instance
    #[must_use]
    pub fn with_port(port: u16, health_checker: Arc<HealthChecker>) -> Self {
        let mut config = MonitoringConfig::default();
        config.port = port;
        Self::new(config, health_checker)
    }

    /// Starts the monitoring HTTP server.
    ///
    /// This method blocks until the server is shut down.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to start or encounters a fatal error.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use infrastructure::monitoring::MonitoringServer;
    /// # use infrastructure::health::HealthChecker;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let health_checker = Arc::new(HealthChecker::new());
    /// let server = MonitoringServer::with_port(8080, health_checker);
    /// server.start().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let health_checker = Arc::clone(&self.health_checker);

        // Build router based on configuration
        let mut router = Router::new();

        if self.config.enable_metrics {
            router = router.route("/metrics", get(metrics_handler));
        }

        if self.config.enable_health {
            router = router
                .route("/health", get(health_handler))
                .route("/health/ready", get(readiness_handler))
                .route("/health/live", get(liveness_handler));
        }

        // Add state to all routes
        let app = router.with_state(health_checker);

        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        info!(
            address = %addr,
            metrics_enabled = self.config.enable_metrics,
            health_enabled = self.config.enable_health,
            "Monitoring server starting"
        );

        // Spawn background task for processing rate updates
        self.spawn_rate_updater();

        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Spawns a background task that periodically updates the processing rate metric.
    fn spawn_rate_updater(&self) {
        let interval = Duration::from_secs(self.config.rate_update_interval_secs);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);

            loop {
                ticker.tick().await;
                update_processing_rate();
                debug!("Updated processing rate metric");
            }
        });
    }
}

// =============================================================================
// HTTP HANDLERS
// =============================================================================

/// Handler for the `/metrics` endpoint.
///
/// Returns Prometheus-formatted metrics.
async fn metrics_handler() -> impl IntoResponse {
    match render_metrics() {
        Ok(metrics) => {
            let response = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/plain; version=0.0.4; charset=utf-8")
                .body(metrics)
                .unwrap_or_else(|_| Response::new("Internal error".to_string()));
            response
        }
        Err(e) => {
            error!(error = %e, "Failed to render metrics");
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body("Internal error".to_string())
                .unwrap_or_else(|_| Response::new("Internal error".to_string()))
        }
    }
}

/// Handler for the `/health` endpoint.
///
/// Returns overall health status including all component statuses.
async fn health_handler(
    State(checker): State<Arc<HealthChecker>>,
) -> impl IntoResponse {
    let health = checker.check_health().await;

    let http_status = match health.status {
        OverallHealth::Healthy => StatusCode::OK,
        OverallHealth::Degraded => StatusCode::OK,
        OverallHealth::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
        OverallHealth::Unknown => StatusCode::OK,
    };

    let response = json!({
        "status": health.status.to_string(),
        "timestamp": health.timestamp,
        "version": health.version,
        "components": health.components,
    });

    (http_status, axum::Json(response))
}

/// Handler for the `/health/ready` readiness probe.
///
/// Returns 200 OK if the system is ready to accept traffic,
/// 503 Service Unavailable otherwise.
///
/// Kubernetes uses this to determine when to add/remove pods from service.
async fn readiness_handler(
    State(checker): State<Arc<HealthChecker>>,
) -> impl IntoResponse {
    let health = checker.check_health().await;

    // Ready if healthy or degraded (can still serve traffic)
    let ready = matches!(
        health.status,
        OverallHealth::Healthy | OverallHealth::Degraded | OverallHealth::Unknown
    );

    let response = json!({
        "ready": ready,
        "status": health.status.to_string(),
        "timestamp": health.timestamp,
        "components": health.components,
    });

    if ready {
        (StatusCode::OK, axum::Json(response))
    } else {
        warn!("Readiness probe failed - system not ready");
        (StatusCode::SERVICE_UNAVAILABLE, axum::Json(response))
    }
}

/// Handler for the `/health/live` liveness probe.
///
/// Returns 200 OK if the service is running.
///
/// Kubernetes uses this to restart unhealthy pods.
async fn liveness_handler(
    State(checker): State<Arc<HealthChecker>>,
) -> axum::Json<serde_json::Value> {
    let alive = checker.is_alive().await;
    let uptime = checker.uptime_secs();

    let response = json!({
        "alive": alive,
        "uptime_secs": uptime,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    axum::Json(response)
}

// =============================================================================
// DATA PIPELINE INTEGRATION
// =============================================================================

/// Integrates metrics collection with the data pipeline.
///
/// This struct provides methods to record metrics from the data pipeline
/// and update health status based on pipeline state.
#[derive(Debug, Clone)]
pub struct PipelineMetricsIntegration {
    health_checker: Arc<HealthChecker>,
}

impl PipelineMetricsIntegration {
    /// Creates a new pipeline metrics integration.
    #[must_use]
    pub fn new(health_checker: Arc<HealthChecker>) -> Self {
        Self { health_checker }
    }

    /// Records that bars have been ingested.
    ///
    /// # Arguments
    ///
    /// * `count` - Number of bars ingested
    pub fn record_bars_ingested(&self, count: u64) {
        crate::metrics::record_bars_ingested(count);
    }

    /// Records that bars have been persisted.
    ///
    /// # Arguments
    ///
    /// * `count` - Number of bars persisted
    pub fn record_bars_persisted(&self, count: u64) {
        crate::metrics::record_bars_persisted(count);
    }

    /// Records a database query latency.
    ///
    /// # Arguments
    ///
    /// * `latency_ms` - Query latency in milliseconds
    pub fn record_db_latency(&self, latency_ms: f64) {
        crate::metrics::record_db_latency(latency_ms);
    }

    /// Records an error.
    pub fn record_error(&self) {
        crate::metrics::record_error();
    }

    /// Updates the provider connection status.
    ///
    /// # Arguments
    ///
    /// * `connected` - `true` if provider is connected
    pub fn set_provider_connected(&self, connected: bool) {
        crate::metrics::set_provider_connected(connected);
    }

    /// Updates the circuit breaker state.
    ///
    /// # Arguments
    ///
    /// * `state` - 0 = closed, 1 = open, 2 = half-open
    pub fn set_circuit_breaker_state(&self, state: u8) {
        crate::metrics::set_circuit_breaker_state(state);
    }

    /// Updates the buffer depth gauge.
    ///
    /// # Arguments
    ///
    /// * `depth` - Current buffer depth
    pub fn set_buffer_depth(&self, depth: usize) {
        crate::metrics::set_buffer_depth(depth);
    }

    /// Updates the active subscribers gauge.
    ///
    /// # Arguments
    ///
    /// * `count` - Number of active subscribers
    pub fn set_active_subscribers(&self, count: usize) {
        crate::metrics::set_active_subscribers(count);
    }

    /// Updates the data pipeline health status.
    ///
    /// # Arguments
    ///
    /// * `healthy` - `true` if pipeline is healthy
    pub async fn update_pipeline_health(&self, healthy: bool) {
        let health = if healthy {
            ComponentHealth::up()
        } else {
            ComponentHealth::down("Pipeline is not running or circuit breaker is open")
        };

        self.health_checker
            .update_component("data_pipeline", health)
            .await;
    }
}

// =============================================================================
// CONVENIENCE FUNCTIONS
// =============================================================================

/// Creates and starts a monitoring server with default configuration.
///
/// # Arguments
///
/// * `port` - Server port
/// * `health_checker` - Health checker instance
///
/// # Errors
///
/// Returns an error if the server fails to start.
pub async fn start_monitoring_server(
    port: u16,
    health_checker: Arc<HealthChecker>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = MonitoringServer::with_port(port, health_checker);
    server.start().await
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitoring_config_default() {
        let config = MonitoringConfig::default();
        assert_eq!(config.port, 8080);
        assert_eq!(config.bind_address, "0.0.0.0");
        assert!(config.enable_metrics);
        assert!(config.enable_health);
    }

    #[test]
    fn test_monitoring_server_creation() {
        let health_checker = Arc::new(HealthChecker::new());
        let config = MonitoringConfig::default();
        let server = MonitoringServer::new(config, health_checker);

        assert_eq!(server.config.port, 8080);
    }

    #[test]
    fn test_monitoring_server_with_port() {
        let health_checker = Arc::new(HealthChecker::new());
        let server = MonitoringServer::with_port(9090, health_checker);

        assert_eq!(server.config.port, 9090);
    }

    #[tokio::test]
    async fn test_pipeline_metrics_integration() {
        let health_checker = Arc::new(HealthChecker::new());
        let integration = PipelineMetricsIntegration::new(Arc::clone(&health_checker));

        // Test that recording metrics doesn't panic
        integration.record_bars_ingested(100);
        integration.record_bars_persisted(50);
        integration.record_db_latency(25.5);
        integration.record_error();
        integration.set_provider_connected(true);
        integration.set_circuit_breaker_state(0);
        integration.set_buffer_depth(10);
        integration.set_active_subscribers(2);

        // Test health update
        integration.update_pipeline_health(true).await;
        let health = health_checker.get_component("data_pipeline").await;
        assert_eq!(health.status, ComponentStatus::Up);
    }
#[tokio::test]
async fn test_health_handler_response() {
    let health_checker = Arc::new(HealthChecker::new());

    health_checker
        .register_component("database", ComponentHealth::up())
        .await;

    // Handler returns impl IntoResponse, verify it doesn't panic
    let _response = health_handler(axum::extract::State(health_checker)).await;
    }

    #[tokio::test]
    async fn test_readiness_handler_ready() {
        let health_checker = Arc::new(HealthChecker::new());

        health_checker
            .register_component("database", ComponentHealth::up())
            .await;

        // Handler returns impl IntoResponse, verify it doesn't panic
        let _response = readiness_handler(axum::extract::State(health_checker)).await;
    }

    #[tokio::test]
    async fn test_readiness_handler_not_ready() {
        let health_checker = Arc::new(HealthChecker::new());

        health_checker
            .register_component("database", ComponentHealth::down("Connection failed"))
            .await;

        // Handler returns impl IntoResponse, verify it doesn't panic
        let _response = readiness_handler(axum::extract::State(health_checker)).await;
    }

    #[tokio::test]
    async fn test_liveness_handler() {
        let health_checker = Arc::new(HealthChecker::new());

        // Handler returns impl IntoResponse, verify it doesn't panic
        let _response = liveness_handler(axum::extract::State(health_checker)).await;
    }
}
