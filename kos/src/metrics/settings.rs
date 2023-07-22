use crate::imports::*;

#[derive(Describe, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum MetricsSettings {
    #[describe("Duration")]
    Duration,
    // #[describe("Terminal font size")]
    // FontSize,
}

#[async_trait]
impl DefaultSettings for MetricsSettings {
    async fn defaults() -> Vec<(Self, String)> {
        vec![]
    }
}
