pub mod data;
#[allow(clippy::module_inception)]
pub mod metrics;

pub use data::{Metric, MetricsData};
pub use metrics::Metrics;
