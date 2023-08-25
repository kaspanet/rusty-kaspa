use crate::imports::*;

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum MetricsSettings {
    Duration,
}

#[async_trait]
impl DefaultSettings for MetricsSettings {
    async fn defaults() -> Vec<(Self, Value)> {
        vec![]
    }
}
