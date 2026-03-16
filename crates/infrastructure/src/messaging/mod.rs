//! # Messaging Infrastructure
//!
//! Async messaging components for event-driven communication between system modules.
//!
//! ## Components
//!
//! - [`EventBus`] - Broadcast-based pub/sub event bus
//! - [`DeadLetterQueue`] - Failed event handling with retry logic
//! - [`Subscription`] - Filtered event subscription handles
//!
//! ## Architecture
//!
//! The messaging layer uses `tokio::sync::broadcast` for efficient multi-consumer
//! event distribution. Key features:
//!
//! - **Lock-free metrics**: Atomic counters for minimal overhead
//! - **Backpressure handling**: Bounded channels prevent memory exhaustion
//! - **Flexible filtering**: Per-subscription event filtering
//! - **Observability**: Integrated tracing and Prometheus metrics

pub use event_bus::{
    DeadLetterQueue, DlqEntry, DlqStorage, EventBus, EventBusMetrics, EventBusSnapshot,
    EventFilter, PrometheusExporter, ResumableSubscription, Subscription,
};

mod event_bus;
