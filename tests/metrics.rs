use std::sync::{Arc, Mutex};

use matrix_bot_sdk::metrics::contexts::MetricContext;
use matrix_bot_sdk::metrics::{IMetricListener, Metrics, decorators};

struct MockMetricListener {
    counters: Mutex<Vec<(String, u64, MetricContext)>>,
    histograms: Mutex<Vec<(String, f64, MetricContext)>>,
}

impl MockMetricListener {
    fn new() -> Self {
        Self {
            counters: Mutex::new(Vec::new()),
            histograms: Mutex::new(Vec::new()),
        }
    }

    fn counters(&self) -> Vec<(String, u64, MetricContext)> {
        self.counters.lock().unwrap().clone()
    }

    fn histograms(&self) -> Vec<(String, f64, MetricContext)> {
        self.histograms.lock().unwrap().clone()
    }
}

impl IMetricListener for MockMetricListener {
    fn on_counter(&self, name: &str, value: u64, ctx: &MetricContext) {
        self.counters
            .lock()
            .unwrap()
            .push((name.to_owned(), value, ctx.clone()));
    }

    fn on_histogram(&self, name: &str, value: f64, ctx: &MetricContext) {
        self.histograms
            .lock()
            .unwrap()
            .push((name.to_owned(), value, ctx.clone()));
    }
}

struct SharedMockMetricListener {
    inner: Arc<MockMetricListener>,
}

impl SharedMockMetricListener {
    fn new() -> (Self, Arc<MockMetricListener>) {
        let inner = Arc::new(MockMetricListener::new());
        (
            Self {
                inner: Arc::clone(&inner),
            },
            inner,
        )
    }

    fn into_trait_object(self) -> Arc<dyn IMetricListener> {
        Arc::new(self)
    }
}

impl IMetricListener for SharedMockMetricListener {
    fn on_counter(&self, name: &str, value: u64, ctx: &MetricContext) {
        self.inner.on_counter(name, value, ctx);
    }

    fn on_histogram(&self, name: &str, value: f64, ctx: &MetricContext) {
        self.inner.on_histogram(name, value, ctx);
    }
}

#[tokio::test]
async fn metrics_can_be_created() {
    let _metrics = Metrics::new();
}

#[tokio::test]
async fn metrics_add_listener() {
    let metrics = Metrics::new();
    let (shared, _mock) = SharedMockMetricListener::new();
    let listener: Arc<dyn IMetricListener> = shared.into_trait_object();

    metrics.add_listener(listener).await;
}

#[tokio::test]
async fn metrics_counter_notifies_listeners() {
    let metrics = Metrics::new();
    let (shared, mock) = SharedMockMetricListener::new();

    metrics.add_listener(shared.into_trait_object()).await;

    let ctx = MetricContext::default();
    metrics.counter("test.counter", 42, ctx).await;

    let counters = mock.counters();
    assert_eq!(counters.len(), 1);
    assert_eq!(counters[0].0, "test.counter");
    assert_eq!(counters[0].1, 42);
}

#[tokio::test]
async fn metrics_counter_with_context() {
    let metrics = Metrics::new();
    let (shared, mock) = SharedMockMetricListener::new();

    metrics.add_listener(shared.into_trait_object()).await;

    let ctx = MetricContext::default().with_tag("room", "!test:example.org");
    metrics.counter("events.received", 1, ctx).await;

    let counters = mock.counters();
    assert_eq!(counters.len(), 1);
    assert!(counters[0].2.tags.contains_key("room"));
}

#[tokio::test]
async fn metrics_histogram_notifies_listeners() {
    let metrics = Metrics::new();
    let (shared, mock) = SharedMockMetricListener::new();

    metrics.add_listener(shared.into_trait_object()).await;

    let ctx = MetricContext::default();
    metrics.histogram("test.histogram", 123.45, ctx).await;

    let histograms = mock.histograms();
    assert_eq!(histograms.len(), 1);
    assert_eq!(histograms[0].0, "test.histogram");
    assert!((histograms[0].1 - 123.45).abs() < 0.001);
}

#[tokio::test]
async fn metrics_multiple_listeners() {
    let metrics = Metrics::new();
    let (shared1, mock1) = SharedMockMetricListener::new();
    let (shared2, mock2) = SharedMockMetricListener::new();

    metrics.add_listener(shared1.into_trait_object()).await;
    metrics.add_listener(shared2.into_trait_object()).await;

    let ctx = MetricContext::default();
    metrics.counter("test.counter", 1, ctx).await;

    assert_eq!(mock1.counters().len(), 1);
    assert_eq!(mock2.counters().len(), 1);
}

#[tokio::test]
async fn metrics_multiple_counters() {
    let metrics = Metrics::new();
    let (shared, mock) = SharedMockMetricListener::new();

    metrics.add_listener(shared.into_trait_object()).await;

    let ctx = MetricContext::default();
    metrics.counter("counter1", 1, ctx.clone()).await;
    metrics.counter("counter2", 2, ctx.clone()).await;
    metrics.counter("counter1", 3, ctx).await;

    let counters = mock.counters();
    assert_eq!(counters.len(), 3);
}

#[tokio::test]
async fn metrics_no_listener_no_panic() {
    let metrics = Metrics::new();

    let ctx = MetricContext::default();
    metrics.counter("test.counter", 1, ctx.clone()).await;
    metrics.histogram("test.histogram", 1.0, ctx).await;
}

#[tokio::test]
async fn decorator_observe_duration() {
    let metrics = Metrics::new();
    let (shared, mock) = SharedMockMetricListener::new();

    metrics.add_listener(shared.into_trait_object()).await;

    let ctx = MetricContext::default();
    let result = decorators::observe_duration(&metrics, "test.duration", ctx, async { 42 }).await;

    assert_eq!(result, 42);

    let histograms = mock.histograms();
    assert_eq!(histograms.len(), 1);
    assert_eq!(histograms[0].0, "test.duration");
    assert!(histograms[0].1 >= 0.0);
}

#[tokio::test]
async fn decorator_observe_duration_async_operation() {
    let metrics = Metrics::new();
    let (shared, mock) = SharedMockMetricListener::new();

    metrics.add_listener(shared.into_trait_object()).await;

    let ctx = MetricContext::default();
    let result = decorators::observe_duration(&metrics, "test.sleep", ctx, async {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        "done"
    })
    .await;

    assert_eq!(result, "done");

    let histograms = mock.histograms();
    assert_eq!(histograms.len(), 1);
    assert!(histograms[0].1 >= 10.0);
}

#[test]
fn metric_context_with_tag() {
    let ctx = MetricContext::default().with_tag("key", "value");

    assert_eq!(ctx.tags.get("key"), Some(&"value".to_owned()));
}

#[test]
fn metric_context_multiple_tags() {
    let ctx = MetricContext::default()
        .with_tag("key1", "value1")
        .with_tag("key2", "value2");

    assert_eq!(ctx.tags.get("key1"), Some(&"value1".to_owned()));
    assert_eq!(ctx.tags.get("key2"), Some(&"value2".to_owned()));
}

#[test]
fn metric_context_default_empty() {
    let ctx = MetricContext::default();

    assert!(ctx.tags.is_empty());
}

// ---------------------------------------------------------------------------
// Parent metric aggregation tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_parent_aggregation_counters_sum_correctly() {
    let metrics = Metrics::new();
    let (shared, mock) = SharedMockMetricListener::new();
    metrics.add_listener(shared.into_trait_object()).await;

    let ctx = MetricContext::default();
    metrics.counter("parent.requests", 1, ctx.clone()).await;
    metrics.counter("parent.requests", 1, ctx.clone()).await;
    metrics.counter("parent.requests", 1, ctx.clone()).await;

    let counters = mock.counters();
    assert_eq!(counters.len(), 3);
    let total: u64 = counters.iter().map(|c| c.1).sum();
    assert_eq!(total, 3);
}

#[tokio::test]
async fn metrics_parent_hierarchical_naming() {
    let metrics = Metrics::new();
    let (shared, mock) = SharedMockMetricListener::new();
    metrics.add_listener(shared.into_trait_object()).await;

    let ctx = MetricContext::default();
    metrics.counter("matrix.rooms.joined", 1, ctx.clone()).await;
    metrics.counter("matrix.rooms.left", 1, ctx.clone()).await;
    metrics
        .counter("matrix.messages.sent", 5, ctx.clone())
        .await;

    let counters = mock.counters();
    assert_eq!(counters.len(), 3);
    assert!(counters.iter().any(|c| c.0 == "matrix.rooms.joined"));
    assert!(counters.iter().any(|c| c.0 == "matrix.rooms.left"));
    assert!(counters.iter().any(|c| c.0 == "matrix.messages.sent"));
}

// ---------------------------------------------------------------------------
// Time-series tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_time_series_histogram_sequence() {
    let metrics = Metrics::new();
    let (shared, mock) = SharedMockMetricListener::new();
    metrics.add_listener(shared.into_trait_object()).await;

    let ctx = MetricContext::default();
    let latencies = [10.0_f64, 15.0, 12.0, 20.0, 8.0];
    for &lat in &latencies {
        metrics.histogram("sync.latency_ms", lat, ctx.clone()).await;
    }

    let histograms = mock.histograms();
    assert_eq!(histograms.len(), 5);
    for (i, &expected) in latencies.iter().enumerate() {
        assert!((histograms[i].1 - expected).abs() < 0.001);
    }
}

#[tokio::test]
async fn metrics_time_series_min_max() {
    let metrics = Metrics::new();
    let (shared, mock) = SharedMockMetricListener::new();
    metrics.add_listener(shared.into_trait_object()).await;

    let ctx = MetricContext::default();
    let values = [100.0_f64, 50.0, 200.0, 25.0, 175.0];
    for &val in &values {
        metrics.histogram("response_time", val, ctx.clone()).await;
    }

    let histograms = mock.histograms();
    let min = histograms.iter().map(|h| h.1).fold(f64::INFINITY, f64::min);
    let max = histograms
        .iter()
        .map(|h| h.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let sum: f64 = histograms.iter().map(|h| h.1).sum();

    assert!((min - 25.0).abs() < 0.001);
    assert!((max - 200.0).abs() < 0.001);
    assert!((sum - 550.0).abs() < 0.001);
}

#[tokio::test]
async fn metrics_time_series_tagged_partition() {
    let metrics = Metrics::new();
    let (shared, mock) = SharedMockMetricListener::new();
    metrics.add_listener(shared.into_trait_object()).await;

    let room_a = MetricContext::default().with_tag("room", "!roomA:example.org");
    let room_b = MetricContext::default().with_tag("room", "!roomB:example.org");

    metrics.counter("events.received", 3, room_a.clone()).await;
    metrics.counter("events.received", 7, room_b.clone()).await;
    metrics.counter("events.received", 2, room_a.clone()).await;

    let counters = mock.counters();
    let room_a_total: u64 = counters
        .iter()
        .filter(|c| c.2.tags.get("room") == Some(&"!roomA:example.org".to_owned()))
        .map(|c| c.1)
        .sum();
    let room_b_total: u64 = counters
        .iter()
        .filter(|c| c.2.tags.get("room") == Some(&"!roomB:example.org".to_owned()))
        .map(|c| c.1)
        .sum();

    assert_eq!(room_a_total, 5);
    assert_eq!(room_b_total, 7);
}
