#[allow(clippy::module_inception)]
pub mod metrics;

pub use kaspa_metrics::data::{Metric, MetricsData, MetricsSnapshot};
pub use metrics::MetricsHandler as Metrics;
