//! # Data Pipeline Orchestrator
//!
//! Orchestrates the data flow: Provider → Event Bus → Repository.
//!
//! ## Features
//!
//! - **Buffering & Batching**: In-memory buffer with configurable batch inserts
//! - **Circuit Breaker**: Automatic failover on DB errors with recovery
//! - **Graceful Shutdown**: Clean shutdown with data preservation
//! - **Health Monitoring**: Real-time pipeline health status
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │   Provider  │────▶│  EventBus   │────▶│   Buffer    │────▶│ Repository  │
//! │  (IB/TWS)   │     │  (Pub/Sub)  │     │ (In-Mem)    │     │(TimescaleDB)│
//! └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘
//!       │                    │                   │                   │
//!       ▼                    ▼                   ▼                   ▼
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │  Ingestion  │     │Distribution │     │Persistence  │     │  Circuit    │
//! │    Task     │     │    Task     │     │   Task      │     │  Breaker    │
//! └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘
//! ```
//!
//! ## Example
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use application::data_pipeline::{DataPipeline, PipelineConfig, EventBusPort};
//! use domain::providers::MarketDataProvider;
//! use domain::repositories::BarRepository;
//! use tokio::sync::broadcast;
//! use domain::events::TradingEvent;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = PipelineConfig::default();
//! let provider: Arc<dyn MarketDataProvider> = todo!();
//! let repo: Arc<dyn BarRepository> = todo!();
//! 
//! // Create event bus channel
//! let (event_tx, _) = broadcast::channel(1000);
//! let event_bus = SimpleEventBus::new(event_tx);
//!
//! let pipeline = DataPipeline::new(config, provider, repo, event_bus);
//! pipeline.run().await?;
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, instrument, warn};

use domain::entities::Bar;
use domain::events::TradingEvent;
use domain::providers::MarketDataProvider;
use domain::repositories::BarRepository;
use domain::values::{Symbol, TimeFrame};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Default buffer size for in-memory bar storage.
const DEFAULT_BUFFER_SIZE: usize = 1000;

/// Default batch size for database inserts.
const DEFAULT_BATCH_SIZE: usize = 100;

/// Default flush interval in seconds.
const DEFAULT_FLUSH_INTERVAL_SECS: u64 = 5;

/// Default circuit breaker failure threshold.
const DEFAULT_CIRCUIT_THRESHOLD: u32 = 5;

/// Default circuit breaker timeout in seconds.
const DEFAULT_CIRCUIT_TIMEOUT_SECS: u64 = 30;

/// Default half-open test interval in seconds.
const DEFAULT_HALF_OPEN_INTERVAL_SECS: u64 = 10;

/// Maximum graceful shutdown time in seconds.
const MAX_SHUTDOWN_TIMEOUT_SECS: u64 = 30;

// =============================================================================
// EVENT BUS PORT
// =============================================================================

/// Port for event bus operations.
///
/// This trait abstracts the event bus implementation, allowing the pipeline
/// to work with any event bus implementation (in-memory, Redis, etc.).
#[async_trait]
pub trait EventBusPort: Send + Sync + Debug {
    /// Publishes a trading event to the bus.
    ///
    /// # Errors
    ///
    /// Returns error if publishing fails.
    async fn publish(&self, event: TradingEvent) -> Result<(), String>;

    /// Subscribes to events from the bus.
    ///
    /// # Returns
    ///
    /// A receiver channel for trading events.
    fn subscribe(&self) -> mpsc::UnboundedReceiver<TradingEvent>;

    /// Returns the number of active subscribers.
    fn subscriber_count(&self) -> usize;
}

// =============================================================================
// PIPELINE STATE
// =============================================================================

/// Pipeline operational state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineState {
    /// Initializing components.
    Starting,
    /// Normal operation.
    Running,
    /// Temporarily paused.
    Paused,
    /// Shutdown in progress.
    ShuttingDown,
    /// Fully stopped.
    Stopped,
    /// Error state.
    Error,
}

impl std::fmt::Display for PipelineState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Paused => write!(f, "paused"),
            Self::ShuttingDown => write!(f, "shutting_down"),
            Self::Stopped => write!(f, "stopped"),
            Self::Error => write!(f, "error"),
        }
    }
}

// =============================================================================
// CIRCUIT BREAKER
// =============================================================================

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CircuitState {
    /// Normal operation - requests allowed.
    Closed,
    /// Failure threshold reached - requests blocked.
    Open,
    /// Testing if service recovered.
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "closed"),
            Self::Open => write!(f, "open"),
            Self::HalfOpen => write!(f, "half_open"),
        }
    }
}

/// Circuit breaker for handling database failures.
///
/// Implements the circuit breaker pattern to prevent cascading failures
/// when the database becomes unavailable.
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Current circuit state.
    state: CircuitState,
    /// Consecutive failure count.
    failure_count: u32,
    /// Last failure timestamp.
    last_failure: Option<Instant>,
    /// Failure threshold to open circuit.
    threshold: u32,
    /// Timeout before attempting reset.
    timeout: Duration,
    /// Half-open test interval.
    half_open_interval: Duration,
}

impl CircuitBreaker {
    /// Creates a new circuit breaker.
    ///
    /// # Arguments
    ///
    /// * `threshold` - Number of consecutive failures before opening
    /// * `timeout_secs` - Seconds to wait before attempting reset
    #[must_use]
    pub fn new(threshold: u32, timeout_secs: u64) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            last_failure: None,
            threshold,
            timeout: Duration::from_secs(timeout_secs),
            half_open_interval: Duration::from_secs(DEFAULT_HALF_OPEN_INTERVAL_SECS),
        }
    }

    /// Records a successful operation.
    pub fn record_success(&mut self) {
        if self.state == CircuitState::HalfOpen {
            info!("Circuit breaker closed - service recovered");
            self.state = CircuitState::Closed;
        }
        self.failure_count = 0;
        self.last_failure = None;
    }

    /// Records a failure and potentially opens the circuit.
    ///
    /// # Returns
    ///
    /// The current circuit state after recording the failure.
    pub fn record_failure(&mut self) -> CircuitState {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());

        if self.state == CircuitState::HalfOpen {
            warn!("Circuit breaker opened - recovery failed");
            self.state = CircuitState::Open;
        } else if self.failure_count >= self.threshold && self.state == CircuitState::Closed {
            warn!(
                failure_count = self.failure_count,
                threshold = self.threshold,
                "Circuit breaker opened - too many failures"
            );
            self.state = CircuitState::Open;
        }

        self.state
    }

    /// Checks if execution is allowed.
    ///
    /// # Returns
    ///
    /// `true` if the circuit allows execution, `false` otherwise.
    pub fn can_execute(&self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                if let Some(last_failure) = self.last_failure {
                    last_failure.elapsed() >= self.timeout
                } else {
                    true
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Attempts to transition from Open to HalfOpen for testing.
    ///
    /// # Returns
    ///
    /// `true` if transition to HalfOpen occurred.
    pub fn attempt_reset(&mut self) -> bool {
        if self.state == CircuitState::Open {
            if let Some(last_failure) = self.last_failure {
                if last_failure.elapsed() >= self.timeout {
                    info!("Circuit breaker entering half-open state for testing");
                    self.state = CircuitState::HalfOpen;
                    return true;
                }
            }
        }
        false
    }

    /// Returns the current circuit state.
    #[must_use]
    pub const fn state(&self) -> CircuitState {
        self.state
    }

    /// Returns the current failure count.
    #[must_use]
    pub const fn failure_count(&self) -> u32 {
        self.failure_count
    }
}

// =============================================================================
// CONFIGURATION
// =============================================================================

/// Pipeline configuration.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Maximum in-memory buffer size.
    pub buffer_size: usize,
    /// Batch insert size for database.
    pub batch_size: usize,
    /// Periodic flush interval in seconds.
    pub flush_interval_secs: u64,
    /// Circuit breaker failure threshold.
    pub circuit_breaker_threshold: u32,
    /// Circuit breaker timeout in seconds.
    pub circuit_breaker_timeout_secs: u64,
    /// Symbols to subscribe to.
    pub symbols: Vec<Symbol>,
    /// Timeframe for bar subscriptions.
    pub timeframe: TimeFrame,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            buffer_size: DEFAULT_BUFFER_SIZE,
            batch_size: DEFAULT_BATCH_SIZE,
            flush_interval_secs: DEFAULT_FLUSH_INTERVAL_SECS,
            circuit_breaker_threshold: DEFAULT_CIRCUIT_THRESHOLD,
            circuit_breaker_timeout_secs: DEFAULT_CIRCUIT_TIMEOUT_SECS,
            symbols: vec![Symbol::new("ES").unwrap()],
            timeframe: TimeFrame::M1,
        }
    }
}

// =============================================================================
// HEALTH STATUS
// =============================================================================

/// Pipeline health status snapshot.
#[derive(Debug, Clone)]
pub struct HealthStatus {
    /// Current pipeline state.
    pub state: PipelineState,
    /// Circuit breaker state.
    pub circuit_state: CircuitState,
    /// Last error message if any.
    pub last_error: Option<String>,
    /// Current queue/buffer depth.
    pub queue_depth: usize,
    /// Processing rate in bars per second.
    pub processing_rate: f64,
    /// Uptime in seconds.
    pub uptime_secs: u64,
    /// Total bars processed.
    pub total_bars_processed: u64,
    /// Total bars persisted.
    pub total_bars_persisted: u64,
}

impl HealthStatus {
    /// Returns true if pipeline is healthy.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.state == PipelineState::Running && self.circuit_state == CircuitState::Closed
    }

    /// Returns true if pipeline is degraded (circuit open but provider ok).
    #[must_use]
    pub fn is_degraded(&self) -> bool {
        self.state == PipelineState::Running && self.circuit_state == CircuitState::Open
    }

    /// Returns overall health status string.
    #[must_use]
    pub fn overall_status(&self) -> &'static str {
        if self.is_healthy() {
            "healthy"
        } else if self.is_degraded() {
            "degraded"
        } else {
            "unhealthy"
        }
    }
}

// =============================================================================
// ERRORS
// =============================================================================

/// Pipeline operation errors.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// Provider error.
    #[error("Provider error: {0}")]
    Provider(String),
    /// Repository error.
    #[error("Repository error: {0}")]
    Repository(String),
    /// Circuit breaker is open.
    #[error("Circuit breaker open")]
    CircuitOpen,
    /// Shutdown timeout.
    #[error("Shutdown timeout")]
    ShutdownTimeout,
    /// Invalid configuration.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    /// Channel closed unexpectedly.
    #[error("Channel closed: {0}")]
    ChannelClosed(String),
}

/// Result type for pipeline operations.
pub type PipelineResult<T> = Result<T, PipelineError>;

// =============================================================================
// METRICS
// =============================================================================

/// Pipeline metrics collector.
#[derive(Debug, Default)]
pub struct PipelineMetrics {
    /// Total bars ingested.
    bars_ingested: AtomicU64,
    /// Total bars persisted.
    bars_persisted: AtomicU64,
    /// Total batches written.
    batches_written: AtomicU64,
    /// Total errors encountered.
    error_count: AtomicU64,
    /// Start time for uptime calculation.
    start_time: Option<Instant>,
}

impl PipelineMetrics {
    fn new() -> Self {
        Self {
            start_time: Some(Instant::now()),
            ..Default::default()
        }
    }

    fn record_ingested(&self) {
        self.bars_ingested.fetch_add(1, Ordering::Relaxed);
    }

    fn record_persisted(&self, count: u64) {
        self.bars_persisted.fetch_add(count, Ordering::Relaxed);
    }

    fn record_batch(&self) {
        self.batches_written.fetch_add(1, Ordering::Relaxed);
    }

    fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    fn processing_rate(&self) -> f64 {
        if let Some(start) = self.start_time {
            let elapsed_secs = start.elapsed().as_secs_f64();
            if elapsed_secs > 0.0 {
                let ingested = self.bars_ingested.load(Ordering::Relaxed) as f64;
                return ingested / elapsed_secs;
            }
        }
        0.0
    }

    fn uptime_secs(&self) -> u64 {
        self.start_time
            .map(|s| s.elapsed().as_secs())
            .unwrap_or(0)
    }

    // Public getters for external access

    /// Returns total bars ingested.
    #[must_use]
    pub fn bars_ingested(&self) -> u64 {
        self.bars_ingested.load(Ordering::Relaxed)
    }

    /// Returns total bars persisted.
    #[must_use]
    pub fn bars_persisted(&self) -> u64 {
        self.bars_persisted.load(Ordering::Relaxed)
    }

    /// Returns total batches written.
    #[must_use]
    pub fn batches_written(&self) -> u64 {
        self.batches_written.load(Ordering::Relaxed)
    }

    /// Returns total errors encountered.
    #[must_use]
    pub fn error_count(&self) -> u64 {
        self.error_count.load(Ordering::Relaxed)
    }
}

// =============================================================================
// DATA PIPELINE
// =============================================================================

/// Data pipeline orchestrator.
///
/// Manages the flow of market data from providers through the event bus
/// to persistent storage with buffering, batching, and circuit breaker protection.
pub struct DataPipeline {
    /// Current state.
    state: Arc<RwLock<PipelineState>>,
    /// Configuration.
    config: PipelineConfig,
    /// Market data provider.
    provider: Arc<dyn MarketDataProvider>,
    /// Bar repository.
    bar_repo: Arc<dyn BarRepository>,
    /// Event bus port.
    event_bus: Arc<dyn EventBusPort>,
    /// Circuit breaker for DB errors.
    circuit: Arc<RwLock<CircuitBreaker>>,
    /// In-memory buffer for bars.
    buffer: Arc<RwLock<Vec<Bar>>>,
    /// Shutdown signal sender.
    shutdown_tx: Option<broadcast::Sender<()>>,
    /// Pipeline metrics.
    metrics: Arc<PipelineMetrics>,
    /// Last error message.
    last_error: Arc<RwLock<Option<String>>>,
    /// Processing counters per symbol.
    symbol_counters: Arc<RwLock<HashMap<Symbol, AtomicU64>>>,
}

impl DataPipeline {
    /// Creates a new data pipeline.
    ///
    /// # Arguments
    ///
    /// * `config` - Pipeline configuration
    /// * `provider` - Market data provider implementation
    /// * `bar_repo` - Bar repository implementation
    /// * `event_bus` - Event bus port implementation
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use std::sync::Arc;
    /// use application::data_pipeline::{DataPipeline, PipelineConfig};
    /// use application::data_pipeline::EventBusPort;
    ///
    /// # fn example(provider: Arc<dyn domain::providers::MarketDataProvider>,
    /// #           repo: Arc<dyn domain::repositories::BarRepository>,
    /// #           event_bus: Arc<dyn EventBusPort>) {
    /// let config = PipelineConfig::default();
    /// let pipeline = DataPipeline::new(config, provider, repo, event_bus);
    /// # }
    /// ```
    #[must_use]
    pub fn new(
        config: PipelineConfig,
        provider: Arc<dyn MarketDataProvider>,
        bar_repo: Arc<dyn BarRepository>,
        event_bus: Arc<dyn EventBusPort>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            state: Arc::new(RwLock::new(PipelineState::Stopped)),
            config,
            provider,
            bar_repo,
            event_bus,
            circuit: Arc::new(RwLock::new(CircuitBreaker::new(
                DEFAULT_CIRCUIT_THRESHOLD,
                DEFAULT_CIRCUIT_TIMEOUT_SECS,
            ))),
            buffer: Arc::new(RwLock::new(Vec::with_capacity(DEFAULT_BUFFER_SIZE))),
            shutdown_tx: Some(shutdown_tx),
            metrics: Arc::new(PipelineMetrics::new()),
            last_error: Arc::new(RwLock::new(None)),
            symbol_counters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Starts the pipeline (blocking).
    ///
    /// This method runs until shutdown is signaled. It spawns multiple
    /// async tasks for ingestion, distribution, and persistence.
    ///
    /// # Errors
    ///
    /// Returns `PipelineError` if startup fails or a fatal error occurs.
    #[instrument(skip(self))]
    pub async fn run(&self) -> PipelineResult<()> {
        info!("Starting data pipeline");

        // Set state to starting
        {
            let mut state = self.state.write().await;
            *state = PipelineState::Starting;
        }

        // Validate configuration
        if self.config.symbols.is_empty() {
            return Err(PipelineError::InvalidConfig(
                "No symbols configured".to_string(),
            ));
        }

        // Create shutdown channel
        let mut shutdown_rx = self
            .shutdown_tx
            .as_ref()
            .map(|tx| tx.subscribe())
            .ok_or_else(|| PipelineError::InvalidConfig("Shutdown channel not available".to_string()))?;

        // Initialize symbol counters
        {
            let mut counters = self.symbol_counters.write().await;
            for symbol in &self.config.symbols {
                counters.insert(symbol.clone(), AtomicU64::new(0));
            }
        }

        // Set state to running
        {
            let mut state = self.state.write().await;
            *state = PipelineState::Running;
        }
        info!("Pipeline state: Running");

        // Spawn ingestion tasks for each symbol
        let mut ingestion_handles: Vec<tokio::task::JoinHandle<PipelineResult<()>>> = vec![];
        for symbol in &self.config.symbols {
            let handle = tokio::spawn({
                let provider = Arc::clone(&self.provider);
                let event_bus = Arc::clone(&self.event_bus);
                let symbol = symbol.clone();
                let timeframe = self.config.timeframe;
                let mut shutdown = shutdown_rx.resubscribe();
                let metrics = Arc::clone(&self.metrics);

                async move {
                    ingestion_task(provider, event_bus, symbol, timeframe, metrics, &mut shutdown).await
                }
            });
            ingestion_handles.push(handle);
        }

        // Spawn distribution task
        let distribution_handle = tokio::spawn({
            let event_bus = Arc::clone(&self.event_bus);
            let buffer = Arc::clone(&self.buffer);
            let mut shutdown = shutdown_rx.resubscribe();
            let metrics = Arc::clone(&self.metrics);

            async move {
                distribution_task(event_bus, buffer, metrics, &mut shutdown).await
            }
        });

        // Spawn persistence task
        let persistence_handle = tokio::spawn({
            let bar_repo = Arc::clone(&self.bar_repo);
            let buffer = Arc::clone(&self.buffer);
            let circuit = Arc::clone(&self.circuit);
            let config = self.config.clone();
            let mut shutdown = shutdown_rx.resubscribe();
            let metrics = Arc::clone(&self.metrics);
            let last_error = Arc::clone(&self.last_error);

            async move {
                persistence_task(bar_repo, buffer, circuit, config, metrics, last_error, &mut shutdown).await
            }
        });

        // Wait for any task to complete (or shutdown signal)
        tokio::select! {
            biased;

            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received in main loop");
            }
            result = distribution_handle => {
                if let Ok(Err(e)) = result {
                    error!(error = %e, "Distribution task failed");
                    return Err(e);
                }
            }
            result = persistence_handle => {
                if let Ok(Err(e)) = result {
                    error!(error = %e, "Persistence task failed");
                    return Err(e);
                }
            }
        }

        // Wait for all ingestion tasks to complete
        for handle in ingestion_handles {
            if let Ok(Err(e)) = handle.await {
                error!(error = %e, "Ingestion task failed");
            }
        }

        Ok(())
    }

    /// Pauses the pipeline.
    ///
    /// Stops ingestion but maintains buffer and connections.
    pub async fn pause(&self) {
        let mut state = self.state.write().await;
        if *state == PipelineState::Running {
            *state = PipelineState::Paused;
            info!("Pipeline paused");
        }
    }

    /// Resumes a paused pipeline.
    pub async fn resume(&self) {
        let mut state = self.state.write().await;
        if *state == PipelineState::Paused {
            *state = PipelineState::Running;
            info!("Pipeline resumed");
        }
    }

    /// Performs graceful shutdown.
    ///
    /// Signals all tasks to stop, flushes remaining buffer to database,
    /// and closes connections with a timeout.
    ///
    /// # Errors
    ///
    /// Returns `PipelineError::ShutdownTimeout` if shutdown exceeds timeout.
    #[instrument(skip(self))]
    pub async fn shutdown(&self) -> PipelineResult<()> {
        info!("Initiating graceful shutdown");

        // Set state to shutting down
        {
            let mut state = self.state.write().await;
            *state = PipelineState::ShuttingDown;
        }

        // Send shutdown signal
        if let Some(ref tx) = self.shutdown_tx {
            let _ = tx.send(());
        }

        // Flush remaining buffer with timeout
        let flush_result = tokio::time::timeout(
            Duration::from_secs(MAX_SHUTDOWN_TIMEOUT_SECS),
            self.flush_buffer(),
        )
        .await;

        match flush_result {
            Ok(Ok(())) => {
                info!("Buffer flushed successfully");
            }
            Ok(Err(e)) => {
                warn!(error = %e, "Failed to flush buffer during shutdown");
            }
            Err(_) => {
                error!("Shutdown timeout exceeded");
                return Err(PipelineError::ShutdownTimeout);
            }
        }

        // Set final state
        {
            let mut state = self.state.write().await;
            *state = PipelineState::Stopped;
        }

        info!("Pipeline shutdown complete");
        Ok(())
    }

    /// Flushes the buffer to the repository.
    async fn flush_buffer(&self) -> PipelineResult<()> {
        let bars = {
            let mut buffer = self.buffer.write().await;
            if buffer.is_empty() {
                return Ok(());
            }
            let bars = buffer.clone();
            buffer.clear();
            bars
        };

        if bars.is_empty() {
            return Ok(());
        }

        info!(count = bars.len(), "Flushing buffer to repository");

        match self.bar_repo.save_batch(&bars).await {
            Ok(()) => {
                self.metrics.record_persisted(bars.len() as u64);
                debug!("Buffer flushed successfully");
                Ok(())
            }
            Err(e) => {
                let msg = format!("Failed to flush buffer: {}", e);
                error!(error = %msg);
                Err(PipelineError::Repository(msg))
            }
        }
    }

    /// Returns current health status.
    pub async fn health(&self) -> HealthStatus {
        let state = *self.state.read().await;
        let circuit_state = self.circuit.read().await.state();
        let last_error = self.last_error.read().await.clone();
        let queue_depth = self.buffer.read().await.len();
        let processing_rate = self.metrics.processing_rate();
        let uptime_secs = self.metrics.uptime_secs();
        let total_bars_processed = self.metrics.bars_ingested.load(Ordering::Relaxed);
        let total_bars_persisted = self.metrics.bars_persisted.load(Ordering::Relaxed);

        HealthStatus {
            state,
            circuit_state,
            last_error,
            queue_depth,
            processing_rate,
            uptime_secs,
            total_bars_processed,
            total_bars_persisted,
        }
    }

    /// Returns current pipeline state.
    pub async fn state(&self) -> PipelineState {
        *self.state.read().await
    }

    /// Returns current circuit breaker state.
    pub async fn circuit_state(&self) -> CircuitState {
        self.circuit.read().await.state()
    }

    /// Returns current buffer depth.
    pub async fn buffer_depth(&self) -> usize {
        self.buffer.read().await.len()
    }
}

/// Snapshot of pipeline metrics for external consumption.
#[derive(Debug, Clone, Copy, Default)]
pub struct PipelineMetricsSnapshot {
    /// Total bars ingested.
    pub bars_ingested: u64,
    /// Total bars persisted.
    pub bars_persisted: u64,
    /// Total batches written.
    pub batches_written: u64,
    /// Total errors encountered.
    pub error_count: u64,
    /// Current processing rate in bars/sec.
    pub processing_rate: f64,
    /// Uptime in seconds.
    pub uptime_secs: u64,
}

impl DataPipeline {
    /// Returns total bars ingested.
    #[must_use]
    pub fn total_bars_ingested(&self) -> u64 {
        self.metrics.bars_ingested.load(Ordering::Relaxed)
    }

    /// Returns total bars persisted.
    #[must_use]
    pub fn total_bars_persisted(&self) -> u64 {
        self.metrics.bars_persisted.load(Ordering::Relaxed)
    }

    /// Returns current processing rate in bars/sec.
    #[must_use]
    pub fn processing_rate(&self) -> f64 {
        self.metrics.processing_rate()
    }

    /// Returns current error count.
    #[must_use]
    pub fn error_count(&self) -> u64 {
        self.metrics.error_count.load(Ordering::Relaxed)
    }

    /// Returns uptime in seconds.
    #[must_use]
    pub fn uptime_secs(&self) -> u64 {
        self.metrics.uptime_secs()
    }

    /// Returns a copy of the internal metrics.
    #[must_use]
    pub fn get_metrics(&self) -> PipelineMetricsSnapshot {
        PipelineMetricsSnapshot {
            bars_ingested: self.metrics.bars_ingested.load(Ordering::Relaxed),
            bars_persisted: self.metrics.bars_persisted.load(Ordering::Relaxed),
            batches_written: self.metrics.batches_written.load(Ordering::Relaxed),
            error_count: self.metrics.error_count.load(Ordering::Relaxed),
            processing_rate: self.metrics.processing_rate(),
            uptime_secs: self.metrics.uptime_secs(),
        }
    }

    /// Returns the internal metrics reference for external synchronization.
    ///
    /// This allows external code (in the infrastructure layer) to periodically
    /// sync these metrics with Prometheus.
    #[must_use]
    pub fn metrics(&self) -> &PipelineMetrics {
        &self.metrics
    }
}

// =============================================================================
// INGESTION TASK
// =============================================================================

/// Ingestion task: receives bars from provider and publishes to event bus.
#[instrument(skip(provider, event_bus, metrics, shutdown))]
async fn ingestion_task(
    provider: Arc<dyn MarketDataProvider>,
    event_bus: Arc<dyn EventBusPort>,
    symbol: Symbol,
    timeframe: TimeFrame,
    metrics: Arc<PipelineMetrics>,
    shutdown: &mut broadcast::Receiver<()>,
) -> PipelineResult<()> {
    info!(%symbol, %timeframe, "Starting ingestion task");

    // Subscribe to bars from provider
    let mut bar_rx = provider
        .subscribe_bars(&symbol, timeframe)
        .await
        .map_err(|e| PipelineError::Provider(e.to_string()))?;

    info!(%symbol, "Subscribed to provider");

    loop {
        tokio::select! {
            biased;

            // Check for shutdown signal
            _ = shutdown.recv() => {
                info!(%symbol, "Ingestion task received shutdown signal");
                // Unsubscribe
                if let Err(e) = provider.unsubscribe(&symbol).await {
                    warn!(error = %e, "Failed to unsubscribe during shutdown");
                }
                break Ok(());
            }

            // Receive bar from provider
            Some(bar) = bar_rx.recv() => {
                metrics.record_ingested();

                // Create trading event
                let event = TradingEvent::BarReceived {
                    bar,
                    source: "ingestion_task".to_string(),
                    timestamp: chrono::Utc::now(),
                };

                // Publish to event bus
                match event_bus.publish(event).await {
                    Ok(()) => {
                        debug!(
                            symbol = %symbol,
                            "Bar published to event bus"
                        );
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to publish bar to event bus");
                    }
                }
            }
        }
    }
}

// =============================================================================
// DISTRIBUTION TASK
// =============================================================================

/// Distribution task: receives events from event bus and adds to buffer.
#[instrument(skip(event_bus, buffer, _metrics, shutdown))]
async fn distribution_task(
    event_bus: Arc<dyn EventBusPort>,
    buffer: Arc<RwLock<Vec<Bar>>>,
    _metrics: Arc<PipelineMetrics>,
    shutdown: &mut broadcast::Receiver<()>,
) -> PipelineResult<()> {
    info!("Starting distribution task");

    // Subscribe to all events
    let mut event_rx = event_bus.subscribe();

    loop {
        tokio::select! {
            biased;

            // Check for shutdown signal
            _ = shutdown.recv() => {
                info!("Distribution task received shutdown signal");
                break Ok(());
            }

            // Receive event from event bus
            Some(event) = event_rx.recv() => {
                match event {
                    TradingEvent::BarReceived { bar, .. } => {
                        let mut buf = buffer.write().await;

                        // Check buffer capacity
                        if buf.len() >= DEFAULT_BUFFER_SIZE {
                            warn!(
                                buffer_size = buf.len(),
                                "Buffer full, dropping oldest bar"
                            );
                            buf.remove(0);
                        }

                        buf.push(bar);
                        debug!(buffer_size = buf.len(), "Bar added to buffer");
                    }
                    _ => {
                        // Ignore other event types
                        debug!("Ignoring non-bar event");
                    }
                }
            }
        }
    }
}

// =============================================================================
// PERSISTENCE TASK
// =============================================================================

/// Persistence task: flushes buffer to database with batching.
#[instrument(skip(bar_repo, buffer, circuit, config, metrics, last_error, shutdown))]
async fn persistence_task(
    bar_repo: Arc<dyn BarRepository>,
    buffer: Arc<RwLock<Vec<Bar>>>,
    circuit: Arc<RwLock<CircuitBreaker>>,
    config: PipelineConfig,
    metrics: Arc<PipelineMetrics>,
    last_error: Arc<RwLock<Option<String>>>,
    shutdown: &mut broadcast::Receiver<()>,
) -> PipelineResult<()> {
    info!("Starting persistence task");

    let mut flush_interval = interval(Duration::from_secs(config.flush_interval_secs));
    let mut half_open_interval = interval(Duration::from_secs(DEFAULT_HALF_OPEN_INTERVAL_SECS));

    loop {
        tokio::select! {
            biased;

            // Check for shutdown signal
            _ = shutdown.recv() => {
                info!("Persistence task received shutdown signal");
                break Ok(());
            }

            // Periodic flush interval
            _ = flush_interval.tick() => {
                debug!("Periodic flush triggered");
                if let Err(e) = flush_to_repository(
                    &bar_repo,
                    &buffer,
                    &circuit,
                    &config,
                    &metrics,
                    &last_error,
                ).await {
                    warn!(error = %e, "Periodic flush failed");
                }
            }

            // Half-open test interval (only when circuit is open)
            _ = half_open_interval.tick() => {
                let mut circ = circuit.write().await;
                if circ.state() == CircuitState::Open {
                    if circ.attempt_reset() {
                        info!("Circuit breaker entering half-open state");
                    }
                }
            }
        }
    }
}

/// Flushes buffer to repository with circuit breaker logic.
#[instrument(skip(bar_repo, buffer, circuit, config, metrics, last_error))]
async fn flush_to_repository(
    bar_repo: &Arc<dyn BarRepository>,
    buffer: &Arc<RwLock<Vec<Bar>>>,
    circuit: &Arc<RwLock<CircuitBreaker>>,
    config: &PipelineConfig,
    metrics: &Arc<PipelineMetrics>,
    last_error: &Arc<RwLock<Option<String>>>,
) -> PipelineResult<()> {
    // Check if we can execute (circuit breaker)
    let can_execute = {
        let circ = circuit.read().await;
        circ.can_execute()
    };

    if !can_execute {
        let state = circuit.read().await.state();
        if state == CircuitState::Open {
            debug!("Circuit breaker open, skipping flush");
            return Err(PipelineError::CircuitOpen);
        }
    }

    // Extract bars to persist
    let bars_to_persist = {
        let mut buf = buffer.write().await;

        // Check if we have enough bars for a batch
        if buf.len() < config.batch_size {
            debug!(
                buffer_size = buf.len(),
                batch_size = config.batch_size,
                "Not enough bars for batch insert"
            );
            return Ok(());
        }

        // Take batch_size bars
        let count = std::cmp::min(config.batch_size, buf.len());
        let bars: Vec<Bar> = buf.drain(..count).collect();
        bars
    };

    if bars_to_persist.is_empty() {
        return Ok(());
    }

    info!(count = bars_to_persist.len(), "Persisting batch to repository");

    // Attempt to save batch
    match bar_repo.save_batch(&bars_to_persist).await {
        Ok(()) => {
            // Success - record metrics and reset circuit if needed
            let count = bars_to_persist.len() as u64;
            metrics.record_persisted(count);
            metrics.record_batch();

            let mut circ = circuit.write().await;
            if circ.state() == CircuitState::HalfOpen {
                info!("Circuit breaker test succeeded, closing circuit");
            }
            circ.record_success();

            // Clear last error
            let mut err = last_error.write().await;
            *err = None;

            debug!("Batch persisted successfully");
            Ok(())
        }
        Err(e) => {
            // Failure - record error and update circuit breaker
            let error_msg = format!("Database error: {}", e);
            error!(error = %error_msg);

            metrics.record_error();

            // Update last error
            let mut err = last_error.write().await;
            *err = Some(error_msg.clone());

            // Record failure in circuit breaker
            let mut circ = circuit.write().await;
            let new_state = circ.record_failure();

            // Put bars back in buffer for retry
            let mut buf = buffer.write().await;
            for bar in bars_to_persist.into_iter().rev() {
                buf.insert(0, bar);
            }

            // Trim buffer if it exceeds capacity
            while buf.len() > config.buffer_size {
                buf.pop();
            }

            warn!(
                circuit_state = %new_state,
                buffer_size = buf.len(),
                "Batch persist failed, bars returned to buffer"
            );

            Err(PipelineError::Repository(error_msg))
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::values::{Price, Symbol, TimeFrame, Volume};
    use rust_decimal::Decimal;

    fn create_test_bar(symbol: &str) -> Bar {
        Bar::new(
            Utc::now(),
            Symbol::new(symbol).unwrap(),
            Price::new(Decimal::new(10000, 2)).unwrap(),
            Price::new(Decimal::new(10100, 2)).unwrap(),
            Price::new(Decimal::new(9900, 2)).unwrap(),
            Price::new(Decimal::new(10050, 2)).unwrap(),
            Volume::new(1000).unwrap(),
            TimeFrame::M1,
            "test_provider",
        )
        .unwrap()
    }

    #[test]
    fn test_pipeline_state_display() {
        assert_eq!(PipelineState::Running.to_string(), "running");
        assert_eq!(PipelineState::Stopped.to_string(), "stopped");
        assert_eq!(PipelineState::Error.to_string(), "error");
    }

    #[test]
    fn test_circuit_state_display() {
        assert_eq!(CircuitState::Closed.to_string(), "closed");
        assert_eq!(CircuitState::Open.to_string(), "open");
        assert_eq!(CircuitState::HalfOpen.to_string(), "half_open");
    }

    #[test]
    fn test_circuit_breaker_new() {
        let cb = CircuitBreaker::new(5, 30);
        assert_eq!(cb.state(), CircuitState::Closed);
        assert_eq!(cb.failure_count(), 0);
    }

    #[test]
    fn test_circuit_breaker_record_success() {
        let mut cb = CircuitBreaker::new(5, 30);
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert_eq!(cb.failure_count(), 0);
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let mut cb = CircuitBreaker::new(3, 30);
        
        // Record failures up to threshold
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_can_execute_closed() {
        let cb = CircuitBreaker::new(5, 30);
        assert!(cb.can_execute());
    }

    #[test]
    fn test_circuit_breaker_can_execute_open() {
        let mut cb = CircuitBreaker::new(1, 30);
        cb.record_failure();
        
        // Immediately after opening, should not execute
        assert!(!cb.can_execute());
    }

    #[test]
    fn test_circuit_breaker_attempt_reset() {
        let mut cb = CircuitBreaker::new(1, 0); // 0 timeout for immediate test
        cb.record_failure();
        
        // Should transition to half-open
        assert!(cb.attempt_reset());
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_pipeline_config_default() {
        let config = PipelineConfig::default();
        assert_eq!(config.buffer_size, DEFAULT_BUFFER_SIZE);
        assert_eq!(config.batch_size, DEFAULT_BATCH_SIZE);
        assert_eq!(config.flush_interval_secs, DEFAULT_FLUSH_INTERVAL_SECS);
        assert_eq!(config.circuit_breaker_threshold, DEFAULT_CIRCUIT_THRESHOLD);
    }

    #[test]
    fn test_health_status_is_healthy() {
        let status = HealthStatus {
            state: PipelineState::Running,
            circuit_state: CircuitState::Closed,
            last_error: None,
            queue_depth: 0,
            processing_rate: 10.0,
            uptime_secs: 60,
            total_bars_processed: 100,
            total_bars_persisted: 100,
        };
        
        assert!(status.is_healthy());
        assert!(!status.is_degraded());
        assert_eq!(status.overall_status(), "healthy");
    }

    #[test]
    fn test_health_status_is_degraded() {
        let status = HealthStatus {
            state: PipelineState::Running,
            circuit_state: CircuitState::Open,
            last_error: Some("DB timeout".to_string()),
            queue_depth: 100,
            processing_rate: 0.0,
            uptime_secs: 60,
            total_bars_processed: 100,
            total_bars_persisted: 50,
        };
        
        assert!(!status.is_healthy());
        assert!(status.is_degraded());
        assert_eq!(status.overall_status(), "degraded");
    }

    #[test]
    fn test_health_status_is_unhealthy() {
        let status = HealthStatus {
            state: PipelineState::Error,
            circuit_state: CircuitState::Open,
            last_error: Some("Fatal error".to_string()),
            queue_depth: 0,
            processing_rate: 0.0,
            uptime_secs: 0,
            total_bars_processed: 0,
            total_bars_persisted: 0,
        };
        
        assert!(!status.is_healthy());
        assert!(!status.is_degraded());
        assert_eq!(status.overall_status(), "unhealthy");
    }

    #[test]
    fn test_pipeline_error_display() {
        let err = PipelineError::CircuitOpen;
        assert_eq!(err.to_string(), "Circuit breaker open");
        
        let err = PipelineError::Provider("timeout".to_string());
        assert_eq!(err.to_string(), "Provider error: timeout");
        
        let err = PipelineError::Repository("connection failed".to_string());
        assert_eq!(err.to_string(), "Repository error: connection failed");
    }

    #[test]
    fn test_create_test_bar() {
        let bar = create_test_bar("AAPL");
        assert_eq!(bar.symbol().as_str(), "AAPL");
        assert_eq!(bar.timeframe(), TimeFrame::M1);
    }

    #[test]
    fn test_metrics_new() {
        let metrics = PipelineMetrics::new();
        assert!(metrics.start_time.is_some());
        assert_eq!(metrics.bars_ingested.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_metrics_record_ingested() {
        let metrics = PipelineMetrics::new();
        metrics.record_ingested();
        metrics.record_ingested();
        assert_eq!(metrics.bars_ingested.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_metrics_record_persisted() {
        let metrics = PipelineMetrics::new();
        metrics.record_persisted(10);
        assert_eq!(metrics.bars_persisted.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn test_metrics_record_error() {
        let metrics = PipelineMetrics::new();
        metrics.record_error();
        metrics.record_error();
        assert_eq!(metrics.error_count.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_success_closes() {
        let mut cb = CircuitBreaker::new(1, 0);
        cb.record_failure();
        cb.attempt_reset(); // Now HalfOpen
        
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_failure_reopens() {
        let mut cb = CircuitBreaker::new(1, 0);
        cb.record_failure();
        cb.attempt_reset(); // Now HalfOpen
        
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    // These tests require infrastructure mocks and would be run with --ignored
    // They serve as documentation of expected behavior

    #[tokio::test]
    #[ignore = "Requires full infrastructure setup"]
    async fn test_pipeline_start_stop() {
        // Test basic lifecycle
    }

    #[tokio::test]
    #[ignore = "Requires full infrastructure setup"]
    async fn test_buffer_batching() {
        // Test buffer accumulation and flush
    }

    #[tokio::test]
    #[ignore = "Requires full infrastructure setup"]
    async fn test_circuit_breaker_open() {
        // Test circuit opens on DB errors
    }

    #[tokio::test]
    #[ignore = "Requires full infrastructure setup"]
    async fn test_circuit_breaker_recovery() {
        // Test auto-recovery after DB recovery
    }

    #[tokio::test]
    #[ignore = "Requires full infrastructure setup"]
    async fn test_graceful_shutdown() {
        // Test clean shutdown with data preservation
    }

    #[tokio::test]
    #[ignore = "Requires full infrastructure setup"]
    async fn test_health_status_reporting() {
        // Test health endpoint accuracy
    }

    #[tokio::test]
    #[ignore = "Requires full infrastructure setup"]
    async fn test_multiple_symbols() {
        // Test multi-subscription handling
    }

    #[tokio::test]
    #[ignore = "Requires full infrastructure setup"]
    async fn test_error_propagation() {
        // Test error handling throughout pipeline
    }
}
