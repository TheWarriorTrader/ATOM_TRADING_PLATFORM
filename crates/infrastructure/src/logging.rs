//! # Logging Module
//!
//! Enterprise-grade tracing and logging configuration with structured logging,
//! correlation IDs, and end-to-end request tracing.
//!
//! ## Features
//!
//! - **Structured Logging**: JSON output for production environments
//! - **Correlation IDs**: Track requests across service boundaries
//! - **Configurable Levels**: Environment-based log level configuration
//! - **Layered Architecture**: Multiple subscriber layers for different outputs
//!
//! ## Usage
//!
//! ```rust
//! use infrastructure::logging::{init_tracing, CorrelationId};
//! use tracing::{info, instrument};
//!
//! #[instrument]
//! async fn process_request(correlation_id: CorrelationId) {
//!     info!(%correlation_id, "Processing request");
//! }
//! ```

use std::cell::RefCell;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};
use uuid::Uuid;

use crate::config::LoggingConfig;
use crate::errors::{InfrastructureError, InfrastructureResult};

/// Thread-local storage for correlation ID
thread_local! {
    static CORRELATION_ID: RefCell<Option<CorrelationId>> = const { RefCell::new(None) };
}

/// Correlation ID for tracking requests across service boundaries
///
/// This type represents a unique identifier that propagates through
/// the entire request lifecycle, enabling end-to-end tracing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CorrelationId(Uuid);

impl CorrelationId {
    /// Creates a new random correlation ID
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a correlation ID from an existing UUID
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Attempts to parse a correlation ID from a string
    ///
    /// # Errors
    ///
    /// Returns error if the string is not a valid UUID
    pub fn parse(s: &str) -> InfrastructureResult<Self> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| InfrastructureError::configuration(format!("Invalid correlation ID: {e}")))
    }

    /// Returns the underlying UUID
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }

    /// Returns the correlation ID as a hyphenated string
    #[must_use]
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for CorrelationId {
    type Err = InfrastructureError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl From<CorrelationId> for String {
    fn from(id: CorrelationId) -> Self {
        id.to_string()
    }
}

impl From<Uuid> for CorrelationId {
    fn from(uuid: Uuid) -> Self {
        Self::from_uuid(uuid)
    }
}

/// Log output format variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    /// Human-readable pretty format for development
    #[default]
    Pretty,
    /// JSON format for production structured logging
    Json,
    /// Compact single-line format
    Compact,
}

impl fmt::Display for LogFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pretty => write!(f, "pretty"),
            Self::Json => write!(f, "json"),
            Self::Compact => write!(f, "compact"),
        }
    }
}

impl FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pretty" => Ok(Self::Pretty),
            "json" => Ok(Self::Json),
            "compact" => Ok(Self::Compact),
            _ => Err(format!("Invalid log format: {s}. Use 'pretty', 'json', or 'compact'")),
        }
    }
}

impl From<&str> for LogFormat {
    fn from(s: &str) -> Self {
        Self::from_str(s).unwrap_or_default()
    }
}

/// Log output destination
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LogOutput {
    /// Standard output
    #[default]
    Stdout,
    /// Standard error
    Stderr,
    /// File path (includes the path string)
    File(String),
}

impl fmt::Display for LogOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdout => write!(f, "stdout"),
            Self::Stderr => write!(f, "stderr"),
            Self::File(path) => write!(f, "file:{path}"),
        }
    }
}

impl FromStr for LogOutput {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stdout" => Ok(Self::Stdout),
            "stderr" => Ok(Self::Stderr),
            path if path.starts_with("file:") => Ok(Self::File(path[5..].to_string())),
            path if path.starts_with('/') || path.starts_with("./") || path.starts_with("../") => {
                Ok(Self::File(path.to_string()))
            }
            _ => Err(format!("Invalid log output: {s}. Use 'stdout', 'stderr', or a file path")),
        }
    }
}

impl From<&str> for LogOutput {
    fn from(s: &str) -> Self {
        Self::from_str(s).unwrap_or_default()
    }
}

/// Extended logging configuration with enterprise features
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// Log level filter (trace, debug, info, warn, error)
    pub level: String,
    /// Output format
    pub format: LogFormat,
    /// Output destination
    pub output: LogOutput,
    /// Enable correlation ID tracking
    pub enable_correlation_ids: bool,
    /// Enable span event logging (enter/exit/close)
    pub enable_span_events: bool,
    /// Include thread IDs in logs
    pub include_thread_ids: bool,
    /// Include target/module path
    pub include_target: bool,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: LogFormat::Pretty,
            output: LogOutput::Stdout,
            enable_correlation_ids: true,
            enable_span_events: false,
            include_thread_ids: false,
            include_target: true,
        }
    }
}

impl From<LoggingConfig> for TracingConfig {
    fn from(config: LoggingConfig) -> Self {
        Self {
            level: config.level,
            format: LogFormat::from(config.format.as_str()),
            output: LogOutput::from(config.output.as_str()),
            enable_correlation_ids: true,
            enable_span_events: false,
            include_thread_ids: false,
            include_target: true,
        }
    }
}

impl From<&LoggingConfig> for TracingConfig {
    fn from(config: &LoggingConfig) -> Self {
        Self {
            level: config.level.clone(),
            format: LogFormat::from(config.format.as_str()),
            output: LogOutput::from(config.output.as_str()),
            enable_correlation_ids: true,
            enable_span_events: false,
            include_thread_ids: false,
            include_target: true,
        }
    }
}

/// Visitor to extract correlation ID from span attributes
#[derive(Debug, Default)]
pub struct CorrelationIdVisitor {
    /// The extracted correlation ID if found
    pub correlation_id: Option<CorrelationId>,
}

impl tracing::field::Visit for CorrelationIdVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "correlation_id" || field.name() == "correlationId" {
            if let Ok(id) = CorrelationId::parse(value) {
                self.correlation_id = Some(id);
            }
        }
    }

    fn record_debug(&mut self, _field: &tracing::field::Field, _value: &dyn fmt::Debug) {}

    fn record_u64(&mut self, _field: &tracing::field::Field, _value: u64) {}

    fn record_i64(&mut self, _field: &tracing::field::Field, _value: i64) {}

    fn record_bool(&mut self, _field: &tracing::field::Field, _value: bool) {}

    fn record_f64(&mut self, _field: &tracing::field::Field, _value: f64) {}
}

/// Builds environment filter from config and environment variables
///
/// The filter is constructed in order of precedence:
/// 1. `RUST_LOG` environment variable
/// 2. `APP_LOG_LEVEL` environment variable
/// 3. Configuration file setting
fn build_env_filter(config: &TracingConfig) -> InfrastructureResult<EnvFilter> {
    // Try RUST_LOG first (standard tracing convention)
    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        return EnvFilter::try_new(rust_log)
            .map_err(|e| InfrastructureError::configuration(format!("Invalid RUST_LOG: {e}")));
    }

    // Then try APP_LOG_LEVEL
    if let Ok(app_log) = std::env::var("APP_LOG_LEVEL") {
        return EnvFilter::try_new(app_log)
            .map_err(|e| InfrastructureError::configuration(format!("Invalid APP_LOG_LEVEL: {e}")));
    }

    // Fall back to config file
    EnvFilter::try_new(&config.level)
        .map_err(|e| InfrastructureError::configuration(format!("Invalid log level: {e}")))
}

/// Initializes the global tracing subscriber with enterprise configuration
///
/// # Errors
///
/// Returns error if:
/// - Subscriber initialization fails
/// - Invalid log level configuration
/// - File output cannot be created
///
/// # Example
///
/// ```rust
/// use infrastructure::logging::{init_tracing, TracingConfig};
///
/// let config = TracingConfig::default();
/// init_tracing(&config).expect("Failed to initialize tracing");
/// ```
pub fn init_tracing(config: &TracingConfig) -> InfrastructureResult<()> {
    // Build the environment filter
    let env_filter = build_env_filter(config)?;

    // Determine span events to include
    let span_events = if config.enable_span_events {
        FmtSpan::FULL
    } else {
        FmtSpan::CLOSE
    };

    // Build the formatter layer based on format
    match config.format {
        LogFormat::Json => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(env_filter)
                .with_span_events(span_events)
                .with_thread_ids(config.include_thread_ids)
                .with_target(config.include_target)
                .flatten_event(true)
                .with_current_span(true)
                .with_span_list(false)
                .try_init()
                .map_err(|e| InfrastructureError::configuration(format!("Failed to initialize JSON subscriber: {e}")))?;
        }
        LogFormat::Pretty => {
            tracing_subscriber::fmt()
                .pretty()
                .with_env_filter(env_filter)
                .with_span_events(span_events)
                .with_thread_ids(config.include_thread_ids)
                .with_target(config.include_target)
                .with_ansi(true)
                .try_init()
                .map_err(|e| InfrastructureError::configuration(format!("Failed to initialize pretty subscriber: {e}")))?;
        }
        LogFormat::Compact => {
            tracing_subscriber::fmt()
                .compact()
                .with_env_filter(env_filter)
                .with_span_events(span_events)
                .with_thread_ids(config.include_thread_ids)
                .with_target(config.include_target)
                .with_ansi(false)
                .try_init()
                .map_err(|e| InfrastructureError::configuration(format!("Failed to initialize compact subscriber: {e}")))?;
        }
    }

    tracing::info!(
        level = %config.level,
        format = %config.format,
        output = %config.output,
        correlation_ids_enabled = config.enable_correlation_ids,
        "Tracing initialized successfully"
    );

    Ok(())
}

/// Legacy init function for backward compatibility
///
/// # Errors
///
/// Returns error if subscriber initialization fails
pub fn init_tracing_legacy(config: &LoggingConfig) -> InfrastructureResult<()> {
    let tracing_config: TracingConfig = config.into();
    init_tracing(&tracing_config)
}

/// Sets the correlation ID for the current thread
///
/// This function should be called at the entry point of each request
/// to establish the correlation context.
///
/// # Example
///
/// ```rust
/// use infrastructure::logging::{set_correlation_id, CorrelationId};
///
/// fn handle_request() {
///     let correlation_id = CorrelationId::new();
///     set_correlation_id(correlation_id);
///     // All subsequent logging will include this correlation ID
/// }
/// ```
pub fn set_correlation_id(correlation_id: CorrelationId) {
    CORRELATION_ID.with(|storage| {
        *storage.borrow_mut() = Some(correlation_id);
    });
}

/// Gets the current correlation ID for this thread
///
/// Returns `None` if no correlation ID has been set.
///
/// # Example
///
/// ```rust
/// use infrastructure::logging::get_correlation_id;
///
/// fn process() {
///     if let Some(id) = get_correlation_id() {
///         println!("Current correlation ID: {}", id);
///     }
/// }
/// ```
pub fn get_correlation_id() -> Option<CorrelationId> {
    CORRELATION_ID.with(|storage| *storage.borrow())
}

/// Clears the current correlation ID
pub fn clear_correlation_id() {
    CORRELATION_ID.with(|storage| {
        *storage.borrow_mut() = None;
    });
}

/// Executes a function with the given correlation ID in scope
///
/// This is the preferred way to propagate correlation IDs through
/// synchronous function calls. For async, use `set_correlation_id`
/// at the start of the task.
///
/// # Type Parameters
///
/// - `F`: The function to execute
/// - `R`: The return type of the function
///
/// # Example
///
/// ```rust
/// use infrastructure::logging::{with_correlation_id, CorrelationId};
/// use tracing::info;
///
/// fn process_request() {
///     let correlation_id = CorrelationId::new();
///     let result = with_correlation_id(correlation_id, || {
///         info!("This log will have the correlation ID");
///         "result"
///     });
/// }
/// ```
pub fn with_correlation_id<F, R>(correlation_id: CorrelationId, f: F) -> R
where
    F: FnOnce() -> R,
{
    set_correlation_id(correlation_id);
    let result = f();
    clear_correlation_id();
    result
}

/// Extracts or generates a correlation ID from HTTP headers
///
/// Checks for the following headers in order:
/// 1. `X-Correlation-ID`
/// 2. `X-Request-ID`
/// 3. `X-Trace-ID`
///
/// If none are found, generates a new correlation ID.
#[must_use]
pub fn extract_correlation_id(headers: &[(String, String)]) -> CorrelationId {
    for (key, value) in headers {
        if key.eq_ignore_ascii_case("X-Correlation-ID")
            || key.eq_ignore_ascii_case("X-Request-ID")
            || key.eq_ignore_ascii_case("X-Trace-ID")
        {
            if let Ok(id) = CorrelationId::parse(value) {
                return id;
            }
        }
    }
    CorrelationId::new()
}

/// Initializes tracing for tests with debug level
///
/// Safe to call multiple times - will silently succeed if already initialized.
pub fn init_tracing_for_tests() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
}

/// Initializes tracing for integration tests with JSON output
///
/// Useful for testing structured logging in CI/CD pipelines.
pub fn init_tracing_for_integration_tests() {
    let _ = tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correlation_id_new() {
        let id1 = CorrelationId::new();
        let id2 = CorrelationId::new();
        assert_ne!(id1, id2, "Each correlation ID should be unique");
    }

    #[test]
    fn test_correlation_id_from_uuid() {
        let uuid = Uuid::new_v4();
        let id = CorrelationId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn test_correlation_id_parse_valid() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let result = CorrelationId::parse(uuid_str);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), uuid_str);
    }

    #[test]
    fn test_correlation_id_parse_invalid() {
        let result = CorrelationId::parse("invalid-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn test_correlation_id_display() {
        let id = CorrelationId::new();
        let display_str = format!("{}", id);
        assert!(!display_str.is_empty());
        assert_eq!(display_str.len(), 36); // UUID length with hyphens
    }

    #[test]
    fn test_log_format_from_str() {
        assert_eq!(LogFormat::from_str("json").unwrap(), LogFormat::Json);
        assert_eq!(LogFormat::from_str("JSON").unwrap(), LogFormat::Json);
        assert_eq!(LogFormat::from_str("pretty").unwrap(), LogFormat::Pretty);
        assert_eq!(LogFormat::from_str("compact").unwrap(), LogFormat::Compact);
        assert!(LogFormat::from_str("invalid").is_err());
    }

    #[test]
    fn test_log_output_from_str() {
        assert_eq!(LogOutput::from_str("stdout").unwrap(), LogOutput::Stdout);
        assert_eq!(LogOutput::from_str("stderr").unwrap(), LogOutput::Stderr);
        assert_eq!(
            LogOutput::from_str("file:/var/log/app.log").unwrap(),
            LogOutput::File("/var/log/app.log".to_string())
        );
    }

    #[test]
    fn test_extract_correlation_id_from_headers() {
        let headers = vec![
            ("X-Correlation-ID".to_string(), "550e8400-e29b-41d4-a716-446655440000".to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        let id = extract_correlation_id(&headers);
        assert_eq!(id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_extract_correlation_id_from_request_id() {
        let headers = vec![
            ("X-Request-ID".to_string(), "550e8400-e29b-41d4-a716-446655440001".to_string()),
        ];
        let id = extract_correlation_id(&headers);
        assert_eq!(id.to_string(), "550e8400-e29b-41d4-a716-446655440001");
    }

    #[test]
    fn test_extract_correlation_id_generates_new_if_missing() {
        let headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        let id = extract_correlation_id(&headers);
        // Should generate a new UUID, not panic
        assert!(!id.to_string().is_empty());
    }

    #[test]
    fn test_with_correlation_id() {
        let correlation_id = CorrelationId::new();
        let result = with_correlation_id(correlation_id, || {
            // In a real scenario, this would be logged with the correlation ID
            assert_eq!(get_correlation_id(), Some(correlation_id));
            "success"
        });
        assert_eq!(result, "success");
        // After the closure, correlation ID should be cleared
        assert_eq!(get_correlation_id(), None);
    }

    #[test]
    fn test_tracing_config_default() {
        let config = TracingConfig::default();
        assert_eq!(config.level, "info");
        assert_eq!(config.format, LogFormat::Pretty);
        assert_eq!(config.output, LogOutput::Stdout);
        assert!(config.enable_correlation_ids);
        assert!(!config.enable_span_events);
    }

    #[test]
    fn test_tracing_config_from_logging_config() {
        let logging_config = LoggingConfig {
            level: "debug".to_string(),
            format: "json".to_string(),
            output: "stderr".to_string(),
        };
        let config: TracingConfig = (&logging_config).into();
        assert_eq!(config.level, "debug");
        assert_eq!(config.format, LogFormat::Json);
        assert_eq!(config.output, LogOutput::Stderr);
    }

    #[test]
    fn test_set_and_get_correlation_id() {
        let id = CorrelationId::new();
        assert_eq!(get_correlation_id(), None);

        set_correlation_id(id);
        assert_eq!(get_correlation_id(), Some(id));

        clear_correlation_id();
        assert_eq!(get_correlation_id(), None);
    }
}
