use std::future::Future;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;

pub mod names {
    pub const SYNC_DURATION_MS: &str = "matrix.sync.duration_ms";
    pub const EVENTS_RECEIVED: &str = "matrix.events.received";
    pub const HTTP_REQUESTS_TOTAL: &str = "matrix.http.requests_total";
    pub const HTTP_ERRORS_TOTAL: &str = "matrix.http.errors_total";
}

pub mod contexts {
    use std::collections::HashMap;

    #[derive(Debug, Clone, Default)]
    pub struct MetricContext {
        pub tags: HashMap<String, String>,
    }

    impl MetricContext {
        pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
            self.tags.insert(key.into(), value.into());
            self
        }
    }
}

pub trait IMetricListener: Send + Sync {
    fn on_counter(&self, name: &str, value: u64, ctx: &contexts::MetricContext);
    fn on_histogram(&self, name: &str, value: f64, ctx: &contexts::MetricContext);
}

#[derive(Default)]
pub struct Metrics {
    listeners: RwLock<Vec<Arc<dyn IMetricListener>>>,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn add_listener(&self, listener: Arc<dyn IMetricListener>) {
        self.listeners.write().await.push(listener);
    }

    pub async fn counter(&self, name: &str, value: u64, ctx: contexts::MetricContext) {
        let listeners = self.listeners.read().await;
        for listener in listeners.iter() {
            listener.on_counter(name, value, &ctx);
        }
    }

    pub async fn histogram(&self, name: &str, value: f64, ctx: contexts::MetricContext) {
        let listeners = self.listeners.read().await;
        for listener in listeners.iter() {
            listener.on_histogram(name, value, &ctx);
        }
    }
}

pub mod decorators {
    use super::*;

    pub async fn observe_duration<F, T>(
        metrics: &Metrics,
        metric_name: &str,
        ctx: contexts::MetricContext,
        future: F,
    ) -> T
    where
        F: Future<Output = T>,
    {
        let started = Instant::now();
        let output = future.await;
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        metrics.histogram(metric_name, ms, ctx).await;
        output
    }
}
