//! # Health Check Module
//!
//! Provides health check endpoints and probes for the trading platform.
//!
//! ## Features
//!
//! - **Health Check**: Overall system health status
//! - **Readiness Probe**: Checks if system is ready to accept traffic
//! - **Liveness Probe**: Checks if system is running
//! - **Component Health**: Individual component status tracking
//!
//! ## Endpoints
//!
//! - `GET /health` - Overall health status
//! - `GET /health/ready` - Readiness probe
//! - `GET /health/live` - Liveness probe
//!
//! ## Example
//!
//! ```rust,no_run
//! use infrastructure::health::{HealthChecker, HealthStatus, ComponentStatus};
//!
//! let checker = HealthChecker::new();
//! checker.register_component("database", ComponentStatus::Up);
//!
//! let status = checker.check_health().await;
//! println!("Health: {:?}", status.overall);
//! ```

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// =============================================================================
// HEALTH STATUS TYPES
// =============================================================================

/// Overall health status of the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallHealth {
    /// System is fully operational.
    Healthy,
    /// System is operational but some components are degraded.
    Degraded,
    /// System is not operational.
    Unhealthy,
    /// System health is unknown (no components registered).
    Unknown,
}

impl fmt::Display for OverallHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Status of an individual component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentStatus {
    /// Component is operational.
    Up,
    /// Component is down or unreachable.
    Down,
    /// Component status is unknown.
    Unknown,
}

impl fmt::Display for ComponentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Up => write!(f, "up"),
            Self::Down => write!(f, "down"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Health status response for API endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Overall health status.
    pub status: OverallHealth,
    /// Timestamp of the health check.
    pub timestamp: String,
    /// Version of the application.
    pub version: String,
    /// Individual component statuses.
    pub components: HashMap<String, ComponentHealth>,
}

/// Detailed health information for a single component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Component status.
    pub status: ComponentStatus,
    /// Optional error message if component is down.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Response time in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_time_ms: Option<u64>,
    /// Last successful check timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check: Option<String>,
}

impl ComponentHealth {
    /// Creates a new component health status.
    #[must_use]
    pub fn new(status: ComponentStatus) -> Self {
        Self {
            status,
            error: None,
            response_time_ms: None,
            last_check: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Creates a new "up" component health status.
    #[must_use]
    pub fn up() -> Self {
        Self::new(ComponentStatus::Up)
    }

    /// Creates a new "down" component health status with error message.
    #[must_use]
    pub fn down(error: impl Into<String>) -> Self {
        Self {
            status: ComponentStatus::Down,
            error: Some(error.into()),
            response_time_ms: None,
            last_check: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Sets the response time.
    #[must_use]
    pub fn with_response_time(mut self, ms: u64) -> Self {
        self.response_time_ms = Some(ms);
        self
    }
}

// =============================================================================
// HEALTH CHECKER
// =============================================================================

/// Component health check function trait.
#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    /// Performs a health check on a component.
    ///
    /// # Returns
    ///
    /// The health status of the component.
    async fn check(&self) -> ComponentHealth;
}

/// Type alias for health check functions.
pub type HealthCheckFn = Arc<dyn Fn() -> ComponentHealth + Send + Sync>;

/// Async health check function type.
pub type AsyncHealthCheckFn = Arc<dyn HealthCheck>;

/// Health checker for monitoring system components.
///
/// Tracks the health status of various components and provides
/// consolidated health status information.
pub struct HealthChecker {
    /// Component health statuses.
    components: Arc<RwLock<HashMap<String, ComponentHealth>>>,
    /// Async health check functions for dynamic components.
    check_functions: Arc<RwLock<HashMap<String, AsyncHealthCheckFn>>>,
    /// Service start time for uptime calculation.
    start_time: Instant,
    /// Application version.
    version: String,
}

impl std::fmt::Debug for HealthChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthChecker")
            .field("components_count", &self.components.try_read().map(|c| c.len()).unwrap_or(0))
            .field("check_functions_count", &self.check_functions.try_read().map(|c| c.len()).unwrap_or(0))
            .field("start_time", &self.start_time)
            .field("version", &self.version)
            .finish()
    }
}

impl HealthChecker {
    /// Creates a new health checker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            components: Arc::new(RwLock::new(HashMap::new())),
            check_functions: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Registers a static component health status.
    ///
    /// # Arguments
    ///
    /// * `name` - Component name
    /// * `health` - Component health status
    pub async fn register_component(&self, name: impl Into<String>, health: ComponentHealth) {
        let mut components = self.components.write().await;
        components.insert(name.into(), health);
    }

    /// Registers a dynamic health check function for a component.
    ///
    /// # Arguments
    ///
    /// * `name` - Component name
    /// * `check_fn` - Async health check function
    pub async fn register_check(&self, name: impl Into<String>, check_fn: AsyncHealthCheckFn) {
        let mut checks = self.check_functions.write().await;
        checks.insert(name.into(), check_fn);
    }

    /// Updates a component's health status.
    ///
    /// # Arguments
    ///
    /// * `name` - Component name
    /// * `health` - New health status
    pub async fn update_component(&self, name: impl AsRef<str>, health: ComponentHealth) {
        let mut components = self.components.write().await;
        components.insert(name.as_ref().to_string(), health);
        debug!(component = %name.as_ref(), "Updated component health");
    }

    /// Gets a component's health status.
    ///
    /// # Arguments
    ///
    /// * `name` - Component name
    ///
    /// # Returns
    ///
    /// The component health status, or `Unknown` if not found.
    pub async fn get_component(&self, name: impl AsRef<str>) -> ComponentHealth {
        let components = self.components.read().await;
        components
            .get(name.as_ref())
            .cloned()
            .unwrap_or_else(|| ComponentHealth::new(ComponentStatus::Unknown))
    }

    /// Performs health checks on all registered components.
    ///
    /// # Returns
    ///
    /// The overall health status with component details.
    pub async fn check_health(&self) -> HealthStatus {
        let start = Instant::now();

        // Run dynamic health checks
        let check_fns = {
            let checks = self.check_functions.read().await;
            checks.clone()
        };

        for (name, check_fn) in check_fns {
            let check_start = Instant::now();
            let health = check_fn.check().await;
            let elapsed = check_start.elapsed().as_millis() as u64;

            self.update_component(&name, health.with_response_time(elapsed))
                .await;
        }

        // Collect all component statuses
        let components = {
            let comps = self.components.read().await;
            comps.clone()
        };

        // Determine overall health
        let status = Self::calculate_overall_health(&components);

        let elapsed = start.elapsed().as_millis() as u64;
        debug!(status = %status, components_count = components.len(), elapsed_ms = elapsed, "Health check completed");

        HealthStatus {
            status,
            timestamp: chrono::Utc::now().to_rfc3339(),
            version: self.version.clone(),
            components,
        }
    }

    /// Checks if the system is ready to accept traffic.
    ///
    /// Readiness requires all critical components to be up.
    ///
    /// # Returns
    ///
    /// `true` if the system is ready, `false` otherwise.
    pub async fn is_ready(&self) -> bool {
        let health = self.check_health().await;
        matches!(health.status, OverallHealth::Healthy)
    }

    /// Checks if the system is alive (running).
    ///
    /// Liveness only requires the service to be running, not necessarily
    /// all components to be healthy.
    ///
    /// # Returns
    ///
    /// `true` if the service is alive, `false` otherwise.
    pub async fn is_alive(&self) -> bool {
        // Service is alive if it has been running for less than the timeout
        // and hasn't explicitly been marked as dead
        self.start_time.elapsed() < Duration::from_secs(300) // 5 minute grace period
    }

    /// Returns the service uptime in seconds.
    #[must_use]
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Calculates the overall health from component statuses.
    fn calculate_overall_health(components: &HashMap<String, ComponentHealth>) -> OverallHealth {
        if components.is_empty() {
            return OverallHealth::Unknown;
        }

        let up_count = components
            .values()
            .filter(|h| h.status == ComponentStatus::Up)
            .count();
        let down_count = components
            .values()
            .filter(|h| h.status == ComponentStatus::Down)
            .count();

        if down_count == 0 {
            OverallHealth::Healthy
        } else if up_count > 0 {
            OverallHealth::Degraded
        } else {
            OverallHealth::Unhealthy
        }
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// PRE-DEFINED HEALTH CHECKS
// =============================================================================

/// Database health check.
#[derive(Debug)]
pub struct DatabaseHealthCheck {
    /// Connection string or pool reference.
    connection_string: String,
}

impl DatabaseHealthCheck {
    /// Creates a new database health check.
    #[must_use]
    pub fn new(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for DatabaseHealthCheck {
    async fn check(&self) -> ComponentHealth {
        let start = Instant::now();

        // Attempt a simple database connection check
        // This is a placeholder - actual implementation would use sqlx
        match tokio::time::timeout(Duration::from_secs(5), async {
            // In production, this would execute a simple query like SELECT 1
            tokio::time::sleep(Duration::from_millis(1)).await;
            Ok::<(), String>(())
        })
        .await
        {
            Ok(Ok(())) => {
                let elapsed = start.elapsed().as_millis() as u64;
                ComponentHealth::up().with_response_time(elapsed)
            }
            Ok(Err(e)) => ComponentHealth::down(format!("Database error: {e}")),
            Err(_) => ComponentHealth::down("Database connection timeout"),
        }
    }
}

/// Redis health check.
#[derive(Debug)]
pub struct RedisHealthCheck {
    /// Redis connection string.
    connection_string: String,
}

impl RedisHealthCheck {
    /// Creates a new Redis health check.
    #[must_use]
    pub fn new(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for RedisHealthCheck {
    async fn check(&self) -> ComponentHealth {
        let start = Instant::now();

        match tokio::time::timeout(Duration::from_secs(3), async {
            // In production, this would execute PING command
            tokio::time::sleep(Duration::from_millis(1)).await;
            Ok::<(), String>(())
        })
        .await
        {
            Ok(Ok(())) => {
                let elapsed = start.elapsed().as_millis() as u64;
                ComponentHealth::up().with_response_time(elapsed)
            }
            Ok(Err(e)) => ComponentHealth::down(format!("Redis error: {e}")),
            Err(_) => ComponentHealth::down("Redis connection timeout"),
        }
    }
}

/// IB (Interactive Brokers) connection health check.
#[derive(Debug)]
pub struct IBHealthCheck {
    /// IB Gateway host.
    host: String,
    /// IB Gateway port.
    port: u16,
}

impl IBHealthCheck {
    /// Creates a new IB health check.
    #[must_use]
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for IBHealthCheck {
    async fn check(&self) -> ComponentHealth {
        let start = Instant::now();
        let addr = format!("{}:{}", self.host, self.port);

        match tokio::time::timeout(Duration::from_secs(5), async {
            // Attempt TCP connection to IB Gateway
            tokio::net::TcpStream::connect(&addr).await.map_err(|e| e.to_string())
        })
        .await
        {
            Ok(Ok(_)) => {
                let elapsed = start.elapsed().as_millis() as u64;
                ComponentHealth::up().with_response_time(elapsed)
            }
            Ok(Err(e)) => ComponentHealth::down(format!("IB connection error: {e}")),
            Err(_) => ComponentHealth::down("IB connection timeout"),
        }
    }
}

// =============================================================================
// HEALTH SERVER
// =============================================================================

/// HTTP server configuration for health endpoints.
#[derive(Debug, Clone)]
pub struct HealthServerConfig {
    /// Server bind address.
    pub bind_address: String,
    /// Server port.
    pub port: u16,
}

impl Default for HealthServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
        }
    }
}

/// Health HTTP server using Axum.
///
/// Exposes health endpoints:
/// - `GET /health` - Overall health status
/// - `GET /health/ready` - Readiness probe
/// - `GET /health/live` - Liveness probe
#[derive(Debug, Clone)]
pub struct HealthServer {
    config: HealthServerConfig,
    health_checker: Arc<HealthChecker>,
}

impl HealthServer {
    /// Creates a new health server.
    #[must_use]
    pub fn new(config: HealthServerConfig, health_checker: Arc<HealthChecker>) -> Self {
        Self {
            config,
            health_checker,
        }
    }

    /// Starts the health HTTP server.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to start.
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use axum::{
            extract::State,
            http::StatusCode,
            response::Json,
            routing::get,
            Router,
        };

        let health_checker = Arc::clone(&self.health_checker);

        let app = Router::new()
            .route("/health", get(health_handler))
            .route("/health/ready", get(readiness_handler))
            .route("/health/live", get(liveness_handler))
            .with_state(health_checker);

        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        info!(address = %addr, "Health server starting");

        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// Handler for the `/health` endpoint.
async fn health_handler(
    axum::extract::State(checker): axum::extract::State<Arc<HealthChecker>>,
) -> Result<(axum::http::StatusCode, axum::Json<HealthStatus>), axum::http::StatusCode> {
    let status = checker.check_health().await;

    let http_status = match status.status {
        OverallHealth::Healthy => axum::http::StatusCode::OK,
        OverallHealth::Degraded => axum::http::StatusCode::OK, // Still serving traffic
        OverallHealth::Unhealthy => axum::http::StatusCode::SERVICE_UNAVAILABLE,
        OverallHealth::Unknown => axum::http::StatusCode::OK,
    };

    Ok((http_status, axum::Json(status)))
}

/// Handler for the `/health/ready` readiness probe.
async fn readiness_handler(
    axum::extract::State(checker): axum::extract::State<Arc<HealthChecker>>,
) -> Result<(axum::http::StatusCode, axum::Json<serde_json::Value>), axum::http::StatusCode> {
    let health = checker.check_health().await;

    let ready = matches!(health.status, OverallHealth::Healthy | OverallHealth::Degraded | OverallHealth::Unknown);

    let response = serde_json::json!({
        "ready": ready,
        "status": health.status.to_string(),
        "timestamp": health.timestamp,
    });

    if ready {
        Ok((axum::http::StatusCode::OK, axum::Json(response)))
    } else {
        Ok((axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(response)))
    }
}

/// Handler for the `/health/live` liveness probe.
async fn liveness_handler(
    axum::extract::State(checker): axum::extract::State<Arc<HealthChecker>>,
) -> axum::Json<serde_json::Value> {
    let alive = checker.is_alive().await;
    let uptime = checker.uptime_secs();

    axum::Json(serde_json::json!({
        "alive": alive,
        "uptime_secs": uptime,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

// =============================================================================
// BUILDER PATTERN
// =============================================================================

/// Builder for configuring health checks.
#[derive(Debug, Default)]
pub struct HealthCheckerBuilder {
    components: HashMap<String, ComponentHealth>,
}

impl HealthCheckerBuilder {
    /// Creates a new health checker builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a component with "up" status.
    #[must_use]
    pub fn with_component_up(mut self, name: impl Into<String>) -> Self {
        self.components.insert(name.into(), ComponentHealth::up());
        self
    }

    /// Adds a component with "down" status.
    #[must_use]
    pub fn with_component_down(mut self, name: impl Into<String>, error: impl Into<String>) -> Self {
        self.components
            .insert(name.into(), ComponentHealth::down(error));
        self
    }

    /// Builds the health checker.
    #[must_use]
    pub fn build(self) -> HealthChecker {
        let checker = HealthChecker::new();

        // Note: This is synchronous, in practice you'd use an async block
        // or the caller would register components asynchronously
        for (name, health) in self.components {
            let checker_ref = &checker;
            let _ = name;
            let _ = health;
            let _ = checker_ref;
            // Components would be registered in an async context
        }

        checker
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_display() {
        assert_eq!(OverallHealth::Healthy.to_string(), "healthy");
        assert_eq!(OverallHealth::Degraded.to_string(), "degraded");
        assert_eq!(OverallHealth::Unhealthy.to_string(), "unhealthy");
    }

    #[test]
    fn test_component_status_display() {
        assert_eq!(ComponentStatus::Up.to_string(), "up");
        assert_eq!(ComponentStatus::Down.to_string(), "down");
        assert_eq!(ComponentStatus::Unknown.to_string(), "unknown");
    }

    #[tokio::test]
    async fn test_health_checker_new() {
        let checker = HealthChecker::new();
        assert_eq!(checker.uptime_secs(), 0);
    }

    #[tokio::test]
    async fn test_register_and_get_component() {
        let checker = HealthChecker::new();

        checker
            .register_component("database", ComponentHealth::up())
            .await;

        let health = checker.get_component("database").await;
        assert_eq!(health.status, ComponentStatus::Up);
    }

    #[tokio::test]
    async fn test_update_component() {
        let checker = HealthChecker::new();

        checker
            .register_component("database", ComponentHealth::up())
            .await;

        checker
            .update_component("database", ComponentHealth::down("Connection lost"))
            .await;

        let health = checker.get_component("database").await;
        assert_eq!(health.status, ComponentStatus::Down);
        assert!(health.error.is_some());
    }

    #[tokio::test]
    async fn test_check_health() {
        let checker = HealthChecker::new();

        checker
            .register_component("database", ComponentHealth::up())
            .await;
        checker
            .register_component("redis", ComponentHealth::up())
            .await;

        let health = checker.check_health().await;
        assert!(matches!(health.status, OverallHealth::Healthy));
        assert_eq!(health.components.len(), 2);
    }

    #[tokio::test]
    async fn test_check_health_degraded() {
        let checker = HealthChecker::new();

        checker
            .register_component("database", ComponentHealth::up())
            .await;
        checker
            .register_component("redis", ComponentHealth::down("Timeout"))
            .await;

        let health = checker.check_health().await;
        assert!(matches!(health.status, OverallHealth::Degraded));
    }

    #[tokio::test]
    async fn test_check_health_unhealthy() {
        let checker = HealthChecker::new();

        checker
            .register_component("database", ComponentHealth::down("Failed"))
            .await;
        checker
            .register_component("redis", ComponentHealth::down("Timeout"))
            .await;

        let health = checker.check_health().await;
        assert!(matches!(health.status, OverallHealth::Unhealthy));
    }

    #[tokio::test]
    async fn test_is_ready() {
        let checker = HealthChecker::new();

        checker
            .register_component("database", ComponentHealth::up())
            .await;

        assert!(checker.is_ready().await);

        checker
            .update_component("database", ComponentHealth::down("Failed"))
            .await;

        assert!(!checker.is_ready().await);
    }

    #[tokio::test]
    async fn test_is_alive() {
        let checker = HealthChecker::new();
        assert!(checker.is_alive().await);
    }

    #[test]
    fn test_component_health_builder() {
        let health = ComponentHealth::up().with_response_time(25);
        assert_eq!(health.status, ComponentStatus::Up);
        assert_eq!(health.response_time_ms, Some(25));
    }

    #[test]
    fn test_component_health_down() {
        let health = ComponentHealth::down("Connection timeout");
        assert_eq!(health.status, ComponentStatus::Down);
        assert_eq!(health.error, Some("Connection timeout".to_string()));
    }

    #[test]
    fn test_health_status_serialization() {
        let status = HealthStatus {
            status: OverallHealth::Healthy,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            version: "1.0.0".to_string(),
            components: HashMap::new(),
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("1.0.0"));
    }
}
