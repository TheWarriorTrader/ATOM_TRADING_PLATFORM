//! # Logging Module
//!
//! Tracing and logging configuration.

use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::config::LoggingConfig;
use crate::errors::InfrastructureResult;

/// Initializes the global tracing subscriber
///
/// # Errors
///
/// Returns error if initialization fails
pub fn init_tracing(config: &LoggingConfig) -> InfrastructureResult<()> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    match config.format.as_str() {
        "json" => {
            let json_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_span_events(FmtSpan::CLOSE);

            tracing_subscriber::registry()
                .with(filter)
                .with(json_layer)
                .try_init()
                .map_err(|e| crate::errors::InfrastructureError::configuration(e.to_string()))?;
        }
        _ => {
            let pretty_layer = tracing_subscriber::fmt::layer()
                .pretty()
                .with_span_events(FmtSpan::CLOSE);

            tracing_subscriber::registry()
                .with(filter)
                .with(pretty_layer)
                .try_init()
                .map_err(|e| crate::errors::InfrastructureError::configuration(e.to_string()))?;
        }
    }

    Ok(())
}

/// Initializes tracing for tests
pub fn init_tracing_for_tests() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
}
