//! # Event Bus
//!
//! Async messaging infrastructure using `tokio::sync::broadcast` for pub/sub
//! communication between components.
//!
//! ## Features
//!
//! - **Broadcast Channel**: Configurable capacity with backpressure handling
//! - **Subscription Management**: Subscribe/unsubscribe with optional filtering
//! - **Topic Filtering**: Subscribe to specific event patterns
//! - **Dead Letter Queue**: Failed message handling with retry logic
//! - **Metrics**: Prometheus-compatible event throughput tracking
//!
//! ## Example
//!
//! ```rust
//! use infrastructure::messaging::EventBus;
//! use domain::events::TradingEvent;
//!
//! # async fn example() {
//! let bus = EventBus::default();
//!
//! // Subscribe to all events
//! let mut rx = bus.subscribe();
//!
//! // Publish an event
//! // bus.publish(event).await;
//!
//! // Receive events
//! // let event = rx.recv().await;
//! # }
//! ```

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::broadcast::{self, error::RecvError, Receiver, Sender};
use tracing::{debug, error, info, instrument, warn};

use domain::events::TradingEvent;

/// Default capacity for the broadcast channel.
const DEFAULT_CHANNEL_CAPACITY: usize = 10_000;

/// Maximum number of retries for dead letter queue.
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Default timeout for DLQ storage operations.
const DLQ_TIMEOUT_MS: u64 = 5_000;

/// Snapshot of EventBus metrics at a point in time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventBusSnapshot {
    /// Total events published.
    pub published: u64,
    /// Events dropped due to lagging receivers.
    pub dropped: u64,
    /// Current active subscriber count.
    pub subscribers: u64,
    /// Events currently queued in the channel.
    pub queued: usize,
    /// Events consumed by all receivers.
    pub consumed: u64,
}

impl fmt::Display for EventBusSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "EventBusMetrics {{ published: {}, consumed: {}, dropped: {}, subscribers: {}, queued: {} }}",
            self.published, self.consumed, self.dropped, self.subscribers, self.queued
        )
    }
}

/// Lock-free metrics for the EventBus.
#[derive(Debug, Default)]
pub struct EventBusMetrics {
    /// Total events published.
    published: AtomicU64,
    /// Events dropped due to lagging receivers.
    dropped: AtomicU64,
    /// Events consumed by all receivers.
    consumed: AtomicU64,
    /// Active subscriber count.
    subscribers: AtomicU64,
}

impl EventBusMetrics {
    /// Creates a new metrics instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments the published counter.
    fn increment_published(&self) {
        self.published.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments the dropped counter.
    fn increment_dropped(&self, count: u64) {
        self.dropped.fetch_add(count, Ordering::Relaxed);
    }

    /// Increments the consumed counter.
    fn increment_consumed(&self) {
        self.consumed.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments the subscriber count.
    fn increment_subscribers(&self) {
        self.subscribers.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrements the subscriber count.
    fn decrement_subscribers(&self) {
        self.subscribers.fetch_sub(1, Ordering::SeqCst);
    }

    /// Returns a snapshot of current metrics.
    #[must_use]
    pub fn snapshot(&self, queued: usize) -> EventBusSnapshot {
        EventBusSnapshot {
            published: self.published.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            subscribers: self.subscribers.load(Ordering::SeqCst),
            queued,
            consumed: self.consumed.load(Ordering::Relaxed),
        }
    }

    /// Returns published count.
    #[must_use]
    pub fn published(&self) -> u64 {
        self.published.load(Ordering::Relaxed)
    }

    /// Returns dropped count.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Returns consumed count.
    #[must_use]
    pub fn consumed(&self) -> u64 {
        self.consumed.load(Ordering::Relaxed)
    }

    /// Returns active subscriber count.
    #[must_use]
    pub fn subscribers(&self) -> u64 {
        self.subscribers.load(Ordering::SeqCst)
    }
}

/// Event filter function type.
pub type EventFilter = Box<dyn Fn(&TradingEvent) -> bool + Send + Sync>;

/// Subscription handle with optional filtering.
///
/// This struct wraps a broadcast receiver and applies an optional
/// filter to incoming events.
pub struct Subscription {
    /// Underlying broadcast receiver.
    receiver: Receiver<TradingEvent>,
    /// Optional filter function.
    filter: Option<EventFilter>,
    /// Metrics reference for tracking consumption.
    metrics: Arc<EventBusMetrics>,
    /// Subscriber ID for logging.
    subscriber_id: u64,
}

impl fmt::Debug for Subscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subscription")
            .field("subscriber_id", &self.subscriber_id)
            .field("has_filter", &self.filter.is_some())
            .finish()
    }
}

impl Subscription {
    /// Creates a new subscription.
    fn new(
        receiver: Receiver<TradingEvent>,
        filter: Option<EventFilter>,
        metrics: Arc<EventBusMetrics>,
        subscriber_id: u64,
    ) -> Self {
        metrics.increment_subscribers();
        debug!(
            subscriber_id = subscriber_id,
            has_filter = filter.is_some(),
            "New subscription created"
        );
        Self {
            receiver,
            filter,
            metrics,
            subscriber_id,
        }
    }

    /// Receives the next event matching the filter.
    ///
    /// This method will skip events that don't match the filter
    /// and continue waiting until a matching event is received.
    ///
    /// # Errors
    ///
    /// Returns `RecvError` if the channel is closed.
    pub async fn recv(&mut self) -> Result<TradingEvent, RecvError> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    if let Some(ref filter) = self.filter {
                        if filter(&event) {
                            self.metrics.increment_consumed();
                            debug!(
                                subscriber_id = self.subscriber_id,
                                event_type = %event.event_type(),
                                "Event received and matched filter"
                            );
                            return Ok(event);
                        }
                        debug!(
                            subscriber_id = self.subscriber_id,
                            event_type = %event.event_type(),
                            "Event filtered out"
                        );
                    } else {
                        self.metrics.increment_consumed();
                        return Ok(event);
                    }
                }
                Err(RecvError::Closed) => {
                    warn!(subscriber_id = self.subscriber_id, "Channel closed");
                    return Err(RecvError::Closed);
                }
                Err(RecvError::Lagged(count)) => {
                    warn!(
                        subscriber_id = self.subscriber_id,
                        dropped_count = count,
                        "Receiver lagged behind, events dropped"
                    );
                    self.metrics.increment_dropped(count);
                    continue;
                }
            }
        }
    }

    /// Tries to receive an event without blocking.
    ///
    /// Returns `Some(event)` if an event is available and matches the filter,
    /// `None` if no event is available or the event doesn't match the filter.
    ///
    /// # Errors
    ///
    /// Returns `RecvError` if the channel is closed or lagged.
    pub fn try_recv(&mut self) -> Result<Option<TradingEvent>, RecvError> {
        loop {
            match self.receiver.try_recv() {
                Ok(event) => {
                    if let Some(ref filter) = self.filter {
                        if filter(&event) {
                            self.metrics.increment_consumed();
                            return Ok(Some(event));
                        }
                        continue;
                    }
                    self.metrics.increment_consumed();
                    return Ok(Some(event));
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => return Ok(None),
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    return Err(RecvError::Closed);
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(count)) => {
                    self.metrics.increment_dropped(count);
                    continue;
                }
            }
        }
    }

    /// Returns the subscriber ID.
    #[must_use]
    pub const fn subscriber_id(&self) -> u64 {
        self.subscriber_id
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.metrics.decrement_subscribers();
        debug!(subscriber_id = self.subscriber_id, "Subscription dropped");
    }
}

/// Entry in the Dead Letter Queue.
#[derive(Debug, Clone)]
pub struct DlqEntry {
    /// The event that failed processing.
    pub event: TradingEvent,
    /// Error description.
    pub error: String,
    /// Number of retry attempts.
    pub retries: u32,
    /// Timestamp when the entry was created.
    pub timestamp: DateTime<Utc>,
}

use chrono::{DateTime, Utc};

/// Storage trait for Dead Letter Queue.
///
/// Implement this trait to persist failed events to Redis, database,
/// or other storage backends.
pub trait DlqStorage: Send + Sync {
    /// Stores a failed event in the DLQ.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage operation fails.
    fn store(
        &self,
        entry: DlqEntry,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>;

    /// Retrieves all entries from the DLQ.
    ///
    /// # Errors
    ///
    /// Returns an error if the retrieval operation fails.
    fn retrieve(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<DlqEntry>, String>> + Send + '_>>;

    /// Removes a processed entry from the DLQ.
    ///
    /// # Errors
    ///
    /// Returns an error if the removal operation fails.
    fn remove(
        &self,
        event_id: uuid::Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>;
}

/// Dead Letter Queue for handling failed events.
///
/// The DLQ captures events that failed processing after the maximum
/// number of retries, allowing for later inspection and reprocessing.
#[derive(Clone)]
pub struct DeadLetterQueue {
    /// Maximum retry attempts before sending to DLQ.
    max_retries: u32,
    /// Optional external storage backend.
    storage: Option<Arc<dyn DlqStorage>>,
    /// In-memory buffer for DLQ entries.
    buffer: Arc<tokio::sync::Mutex<Vec<DlqEntry>>>,
}

impl fmt::Debug for DeadLetterQueue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeadLetterQueue")
            .field("max_retries", &self.max_retries)
            .field("has_storage", &self.storage.is_some())
            .finish_non_exhaustive()
    }
}

impl DeadLetterQueue {
    /// Creates a new DLQ with default retry limit.
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            storage: None,
            buffer: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    /// Creates a new DLQ with custom retry limit.
    #[must_use]
    pub fn with_retries(max_retries: u32) -> Self {
        Self {
            max_retries,
            storage: None,
            buffer: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    /// Sets the external storage backend.
    pub fn with_storage(mut self, storage: Arc<dyn DlqStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Adds an entry to the DLQ.
    ///
    /// If external storage is configured, the entry will also be persisted there.
    pub async fn add(&self, event: TradingEvent, error: impl Into<String>, retries: u32) {
        let entry = DlqEntry {
            event,
            error: error.into(),
            retries,
            timestamp: Utc::now(),
        };

        warn!(
            event_type = %entry.event.event_type(),
            retries = entry.retries,
            "Event moved to Dead Letter Queue"
        );

        // Add to in-memory buffer
        {
            let mut buffer = self.buffer.lock().await;
            buffer.push(entry.clone());
        }

        // Persist to external storage if available
        if let Some(ref storage) = self.storage {
            let timeout = Duration::from_millis(DLQ_TIMEOUT_MS);
            match tokio::time::timeout(timeout, storage.store(entry)).await {
                Ok(Ok(())) => {
                    debug!("DLQ entry persisted to external storage");
                }
                Ok(Err(e)) => {
                    error!(error = %e, "Failed to persist DLQ entry");
                }
                Err(_) => {
                    error!("DLQ storage operation timed out");
                }
            }
        }
    }

    /// Retrieves all entries from the DLQ.
    pub async fn entries(&self) -> Vec<DlqEntry> {
        self.buffer.lock().await.clone()
    }

    /// Clears all entries from the in-memory buffer.
    pub async fn clear(&self) {
        self.buffer.lock().await.clear();
        info!("Dead Letter Queue cleared");
    }

    /// Returns the maximum retry limit.
    #[must_use]
    pub const fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// Returns the current buffer size.
    pub async fn len(&self) -> usize {
        self.buffer.lock().await.len()
    }

    /// Returns true if the buffer is empty.
    pub async fn is_empty(&self) -> bool {
        self.buffer.lock().await.is_empty()
    }
}

impl Default for DeadLetterQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Async EventBus for pub/sub messaging.
///
/// The EventBus uses a tokio broadcast channel to distribute events
/// to multiple subscribers efficiently. It supports:
///
/// - Multiple concurrent subscribers
/// - Optional per-subscription filtering
/// - Lock-free metrics collection
/// - Dead Letter Queue for failed events
///
/// # Clone Behavior
///
/// Cloning the EventBus creates a new reference to the same underlying
/// channel. All clones share the same event stream and metrics.
#[derive(Debug, Clone)]
pub struct EventBus {
    /// Broadcast channel sender.
    sender: Sender<TradingEvent>,
    /// Shared metrics.
    metrics: Arc<EventBusMetrics>,
    /// Subscriber ID counter.
    next_subscriber_id: Arc<AtomicU64>,
    /// Optional Dead Letter Queue.
    dlq: Option<Arc<DeadLetterQueue>>,
}

impl EventBus {
    /// Creates a new EventBus with the specified channel capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of events to buffer in the channel.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use infrastructure::messaging::EventBus;
    ///
    /// let bus = EventBus::new(1000);
    /// ```
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            metrics: Arc::new(EventBusMetrics::new()),
            next_subscriber_id: Arc::new(AtomicU64::new(1)),
            dlq: None,
        }
    }

    /// Creates a new EventBus with default capacity.
    ///
    /// The default capacity is 10,000 events.
    #[must_use]
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_CHANNEL_CAPACITY)
    }

    /// Attaches a Dead Letter Queue to this EventBus.
    pub fn with_dlq(mut self, dlq: DeadLetterQueue) -> Self {
        self.dlq = Some(Arc::new(dlq));
        self
    }

    /// Publishes an event to all subscribers.
    ///
    /// This method is non-blocking. If the channel is full, the oldest
    /// event will be dropped to make room (broadcast channel behavior).
    ///
    /// # Arguments
    ///
    /// * `event` - The TradingEvent to publish.
    ///
    /// # Returns
    ///
    /// Returns the number of active receivers that received the event.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use domain::events::TradingEvent;
    /// # use domain::entities::Bar;
    /// # use infrastructure::messaging::EventBus;
    /// # fn create_event() -> TradingEvent { unimplemented!() }
    /// # async fn example() {
    /// let bus = EventBus::default();
    /// let event = create_event();
    /// let receiver_count = bus.publish(event);
    /// # }
    /// ```
    #[instrument(skip(self, event), fields(event_type = %event.event_type()))]
    pub fn publish(&self, event: TradingEvent) -> usize {
        let receiver_count = self.sender.send(event).unwrap_or(0);
        self.metrics.increment_published();

        if receiver_count > 0 {
            debug!(
                receiver_count = receiver_count,
                "Event published successfully"
            );
        } else {
            warn!("Event published but no active receivers");
        }

        receiver_count
    }

    /// Subscribes to all events.
    ///
    /// Returns a broadcast receiver that will receive all events published
    /// after the subscription is created.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use infrastructure::messaging::EventBus;
    /// # async fn example() {
    /// let bus = EventBus::default();
    /// let mut rx = bus.subscribe();
    /// // Use rx.recv().await to receive events
    /// # }
    /// ```
    pub fn subscribe(&self) -> Receiver<TradingEvent> {
        let subscriber_id = self.next_subscriber_id.fetch_add(1, Ordering::SeqCst);
        let rx = self.sender.subscribe();
        self.metrics.increment_subscribers();

        info!(
            subscriber_id = subscriber_id,
            "New subscriber added (all events)"
        );

        rx
    }

    /// Subscribes with a filter function.
    ///
    /// Returns a `Subscription` that only yields events matching the filter.
    ///
    /// # Type Parameters
    ///
    /// * `F` - Filter function type that implements `Fn(&TradingEvent) -> bool`.
    ///
    /// # Arguments
    ///
    /// * `filter` - Function that returns true for events to receive.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use infrastructure::messaging::EventBus;
    /// # use domain::events::TradingEvent;
    /// # async fn example() {
    /// let bus = EventBus::default();
    ///
    /// // Subscribe only to market data events
    /// let mut sub = bus.subscribe_filtered(|event: &TradingEvent| {
    ///     event.is_market_data()
    /// });
    ///
    /// // Only BarReceived and TickReceived events will be received
    /// # }
    /// ```
    pub fn subscribe_filtered<F>(&self, filter: F) -> Subscription
    where
        F: Fn(&TradingEvent) -> bool + Send + Sync + 'static,
    {
        let subscriber_id = self.next_subscriber_id.fetch_add(1, Ordering::SeqCst);
        let receiver = self.sender.subscribe();

        info!(
            subscriber_id = subscriber_id,
            "New subscriber added (filtered)"
        );

        Subscription::new(
            receiver,
            Some(Box::new(filter)),
            Arc::clone(&self.metrics),
            subscriber_id,
        )
    }

    /// Subscribes to events matching a topic pattern.
    ///
    /// Supports exact matches and wildcards:
    /// - `"marketdata"` - Matches events with topic "marketdata"
    /// - `"marketdata.*"` - Matches "marketdata.bars", "marketdata.ticks", etc.
    /// - `"*.orders"` - Matches all "orders" subtopics
    ///
    /// # Arguments
    ///
    /// * `pattern` - Topic pattern to match.
    ///
    /// # Note
    ///
    /// Topic matching is currently based on event type strings. Future
    /// versions may support explicit topic tags on events.
    pub fn subscribe_to_topic(&self, pattern: &str) -> Subscription {
        let pattern_owned = pattern.to_string();
        let subscriber_id = self.next_subscriber_id.fetch_add(1, Ordering::SeqCst);
        let receiver = self.sender.subscribe();

        let filter = move |event: &TradingEvent| {
            let event_type = event.event_type();
            if pattern_owned.ends_with(".*") {
                let prefix = &pattern_owned[..pattern_owned.len() - 1];
                event_type.starts_with(prefix)
            } else if pattern_owned.starts_with("*.") {
                let suffix = &pattern_owned[2..];
                event_type.ends_with(suffix)
            } else {
                event_type == pattern_owned
            }
        };

        info!(
            subscriber_id = subscriber_id,
            pattern = %pattern,
            "New subscriber added (topic)"
        );

        Subscription::new(
            receiver,
            Some(Box::new(filter)),
            Arc::clone(&self.metrics),
            subscriber_id,
        )
    }

    /// Returns the number of active receivers.
    #[must_use]
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Returns a snapshot of current metrics.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use infrastructure::messaging::EventBus;
    /// # fn example() {
    /// let bus = EventBus::default();
    /// let metrics = bus.metrics();
    /// println!("Published: {}", metrics.published);
    /// # }
    /// ```
    #[must_use]
    pub fn metrics(&self) -> EventBusSnapshot {
        self.metrics.snapshot(self.sender.len())
    }

    /// Returns the shared metrics reference.
    #[must_use]
    pub fn metrics_ref(&self) -> Arc<EventBusMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Returns the Dead Letter Queue if configured.
    #[must_use]
    pub fn dlq(&self) -> Option<Arc<DeadLetterQueue>> {
        self.dlq.clone()
    }

    /// Creates a receiver that can be used with `select!` macros.
    ///
    /// This is useful when you need to multiplex the event stream
    /// with other async operations.
    pub fn subscribe_resumable(&self) -> ResumableSubscription {
        ResumableSubscription::new(self.subscribe(), Arc::clone(&self.metrics))
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::with_default_capacity()
    }
}

/// A resumable subscription that handles lag errors automatically.
///
/// This wrapper automatically resumes receiving after a lag error,
/// logging a warning about the dropped events.
pub struct ResumableSubscription {
    receiver: Receiver<TradingEvent>,
    metrics: Arc<EventBusMetrics>,
}

impl ResumableSubscription {
    /// Creates a new resumable subscription.
    fn new(receiver: Receiver<TradingEvent>, metrics: Arc<EventBusMetrics>) -> Self {
        Self { receiver, metrics }
    }

    /// Receives the next event, automatically handling lag errors.
    ///
    /// # Errors
    ///
    /// Only returns an error if the channel is closed.
    pub async fn recv(&mut self) -> Result<TradingEvent, RecvError> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    self.metrics.increment_consumed();
                    return Ok(event);
                }
                Err(RecvError::Closed) => return Err(RecvError::Closed),
                Err(RecvError::Lagged(count)) => {
                    warn!(
                        dropped_count = count,
                        "Resumable subscription lagged, continuing..."
                    );
                    self.metrics.increment_dropped(count);
                    continue;
                }
            }
        }
    }
}

/// Metrics exporter for Prometheus-compatible monitoring.
///
/// This struct provides formatted metrics that can be exposed
/// via a Prometheus scrape endpoint.
#[derive(Debug)]
pub struct PrometheusExporter {
    event_bus: EventBus,
}

impl PrometheusExporter {
    /// Creates a new exporter for the given EventBus.
    #[must_use]
    pub fn new(event_bus: EventBus) -> Self {
        Self { event_bus }
    }

    /// Returns metrics in Prometheus exposition format.
    #[must_use]
    pub fn export(&self) -> String {
        let metrics = self.event_bus.metrics();
        let timestamp = Utc::now().timestamp_millis();

        format!(
            r#"# HELP eventbus_events_published_total Total events published
# TYPE eventbus_events_published_total counter
eventbus_events_published_total {{}} {published} {timestamp}

# HELP eventbus_events_consumed_total Total events consumed
# TYPE eventbus_events_consumed_total counter
eventbus_events_consumed_total {{}} {consumed} {timestamp}

# HELP eventbus_events_dropped_total Total events dropped (lagging receivers)
# TYPE eventbus_events_dropped_total counter
eventbus_events_dropped_total {{}} {dropped} {timestamp}

# HELP eventbus_subscribers_active Current active subscribers
# TYPE eventbus_subscribers_active gauge
eventbus_subscribers_active {{}} {subscribers} {timestamp}

# HELP eventbus_events_queued Current events queued in channel
# TYPE eventbus_events_queued gauge
eventbus_events_queued {{}} {queued} {timestamp}
"#,
            published = metrics.published,
            consumed = metrics.consumed,
            dropped = metrics.dropped,
            subscribers = metrics.subscribers,
            queued = metrics.queued,
            timestamp = timestamp
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::Bar;
    use domain::values::{Price, Volume};
    use rust_decimal::dec;
    use std::time::{Duration, Instant};
    use tokio::time::timeout;

    fn create_test_bar() -> Bar {
        Bar::new(
            Utc::now(),
            domain::values::Symbol::new("AAPL").expect("Valid symbol"),
            Price::new(dec!(150.0)).expect("Valid price"),
            Price::new(dec!(151.0)).expect("Valid price"),
            Price::new(dec!(149.0)).expect("Valid price"),
            Price::new(dec!(150.5)).expect("Valid price"),
            Volume::new(1000).expect("Valid volume"),
            domain::values::TimeFrame::parse("1m").expect("Valid timeframe"),
            "test",
        )
        .expect("Valid bar")
    }

    fn create_test_event() -> TradingEvent {
        TradingEvent::bar_received(create_test_bar(), "test")
    }

    #[tokio::test]
    async fn test_publish_subscribe() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe();

        let event = create_test_event();
        let event_type = event.event_type().to_string();
        bus.publish(event);

        let received = timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("Timeout")
            .expect("Recv error");

        assert_eq!(received.event_type(), event_type);
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::default();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        let event = create_test_event();
        bus.publish(event);

        let received1 = timeout(Duration::from_millis(100), rx1.recv())
            .await
            .expect("Timeout")
            .expect("Recv error");

        let received2 = timeout(Duration::from_millis(100), rx2.recv())
            .await
            .expect("Timeout")
            .expect("Recv error");

        assert_eq!(received1.event_type(), received2.event_type());
    }

    #[tokio::test]
    async fn test_filtered_subscription() {
        let bus = EventBus::default();

        // Subscribe only to market data events
        let mut sub = bus.subscribe_filtered(|event| event.is_market_data());

        // Create and publish a market data event
        let bar_event = TradingEvent::bar_received(create_test_bar(), "test");
        bus.publish(bar_event);

        // Should receive the market data event
        let received = timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("Timeout")
            .expect("Recv error");
        assert!(received.is_market_data());
    }

    #[tokio::test]
    async fn test_filtered_subscription_filters_out() {
        let bus = EventBus::default();

        // Subscribe only to order events
        let mut order_sub = bus.subscribe_filtered(|event| event.is_order_event());

        // Publish market data event
        let bar_event = TradingEvent::bar_received(create_test_bar(), "test");
        bus.publish(bar_event);

        // Publish order event
        use domain::entities::{Order, OrderSide, OrderType};
        use domain::values::OrderId;

        let order = Order::new(
            domain::entities::EntityId::new_v4(),
            domain::values::Symbol::new("AAPL").expect("Valid symbol"),
            OrderSide::Buy,
            OrderType::Market,
            domain::values::Quantity::new(dec!(100)).expect("Valid quantity"),
            None,
            None,
        )
        .expect("Valid order");
        let order_event = TradingEvent::order_submitted(OrderId::generate(), order);
        bus.publish(order_event);

        // Should only receive the order event
        let received = timeout(Duration::from_millis(100), order_sub.recv())
            .await
            .expect("Timeout")
            .expect("Recv error");
        assert!(received.is_order_event());
    }

    #[tokio::test]
    async fn test_metrics() {
        let bus = EventBus::default();
        let _rx = bus.subscribe();

        // Publish some events
        for _ in 0..5 {
            bus.publish(create_test_event());
        }

        let metrics = bus.metrics();
        assert_eq!(metrics.published, 5);
        assert_eq!(metrics.subscribers, 1);
    }

    #[tokio::test]
    async fn test_receiver_count() {
        let bus = EventBus::default();
        assert_eq!(bus.receiver_count(), 0);

        let _rx1 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);

        let _rx2 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 2);

        drop(_rx1);
        // Need to yield for drop to take effect
        tokio::task::yield_now().await;
        assert_eq!(bus.receiver_count(), 1);
    }

    #[tokio::test]
    async fn test_try_recv() {
        let bus = EventBus::default();
        let mut sub = bus.subscribe_filtered(|_| true);

        // Should return None when no events
        assert!(sub.try_recv().unwrap().is_none());

        // Publish an event
        bus.publish(create_test_event());

        // Should now receive the event
        let received = sub.try_recv().expect("Try recv failed");
        assert!(received.is_some());
    }

    #[tokio::test]
    async fn test_dead_letter_queue() {
        let dlq = DeadLetterQueue::with_retries(3);
        let dlq_arc = Arc::new(dlq);

        let event = create_test_event();
        dlq_arc
            .add(event.clone(), "Test error", 3)
            .await;

        assert_eq!(dlq_arc.len().await, 1);

        let entries = dlq_arc.entries().await;
        assert_eq!(entries[0].error, "Test error");
        assert_eq!(entries[0].retries, 3);

        dlq_arc.clear().await;
        assert!(dlq_arc.is_empty().await);
    }

    #[tokio::test]
    async fn test_prometheus_exporter() {
        let bus = EventBus::default();
        let exporter = PrometheusExporter::new(bus.clone());

        // Publish some events
        let _rx = bus.subscribe();
        for _ in 0..10 {
            bus.publish(create_test_event());
        }

        let output = exporter.export();
        assert!(output.contains("eventbus_events_published_total"));
        assert!(output.contains("eventbus_subscribers_active"));
        assert!(output.contains("10")); // Should contain our published count
    }

    #[tokio::test]
    async fn test_topic_subscription() {
        let bus = EventBus::default();

        // Subscribe to bar_received events using topic pattern
        let mut sub = bus.subscribe_to_topic("bar_received");

        // Publish different event types
        let bar_event = TradingEvent::bar_received(create_test_bar(), "test");
        bus.publish(bar_event);

        // Create a tick using the constructor
        let tick = domain::entities::Tick::new(
            Utc::now(),
            domain::values::Symbol::new("AAPL").expect("Valid symbol"),
            Price::new(dec!(150.0)).expect("Valid price"),
            domain::values::Volume::new(100).expect("Valid volume"),
            None,
            "test",
        )
        .expect("Valid tick");
        let tick_event = TradingEvent::tick_received(tick, "test");
        bus.publish(tick_event);

        // Should only receive bar event
        let received = timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("Timeout")
            .expect("Recv error");
        assert_eq!(received.event_type(), "bar_received");
    }

    #[tokio::test]
    async fn test_resumable_subscription() {
        let bus = EventBus::new(2); // Small capacity
        let mut sub = bus.subscribe_resumable();

        // Publish events
        for _ in 0..3 {
            bus.publish(create_test_event());
        }

        // Should receive events (may skip some due to lag)
        let received = timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("Timeout")
            .expect("Recv error");
        assert!(received.is_market_data());
    }

    #[tokio::test]
    async fn test_subscription_metrics_on_drop() {
        let bus = EventBus::default();

        {
            let _sub = bus.subscribe_filtered(|_| true);
            assert_eq!(bus.metrics().subscribers, 1);
        }

        // Need to yield for drop to take effect
        tokio::task::yield_now().await;
        assert_eq!(bus.metrics().subscribers, 0);
    }

    #[tokio::test]
    async fn test_high_throughput() {
        use std::sync::atomic::AtomicUsize;

        // Use larger capacity for high throughput test
        let bus = EventBus::new(50_000);
        let received_count = Arc::new(AtomicUsize::new(0));
        let received_count_clone = Arc::clone(&received_count);

        // Spawn subscriber
        let mut rx = bus.subscribe();
        let subscriber = tokio::spawn(async move {
            while timeout(Duration::from_millis(500), rx.recv())
                .await
                .is_ok()
            {
                received_count_clone.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Give subscriber time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Publish many events
        const EVENT_COUNT: usize = 10_000;
        let start = Instant::now();

        for _ in 0..EVENT_COUNT {
            bus.publish(create_test_event());
        }

        let elapsed = start.elapsed();
        let publish_rate = EVENT_COUNT as f64 / elapsed.as_secs_f64();

        // Wait for subscriber to process
        tokio::time::sleep(Duration::from_millis(500)).await;
        subscriber.abort();

        let received = received_count.load(Ordering::Relaxed);
        let metrics = bus.metrics();

        println!(
            "Published: {}, Received: {}, Dropped: {}, Rate: {:.0} events/sec",
            EVENT_COUNT, received, metrics.dropped, publish_rate
        );

        // Should have published all events
        assert_eq!(metrics.published, EVENT_COUNT as u64);
        // Verify reasonable throughput (> 1000 events/sec)
        assert!(publish_rate > 1000.0, "Publish rate too low: {:.0}", publish_rate);
        // At least some events should be received
        assert!(received > 100, "Too few events received: {}", received);
    }
}
