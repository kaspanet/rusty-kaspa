use crate::imports::*;

#[derive(Describe, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum CoreSettings {
    #[describe("Metrics")]
    Metrics,
    // #[describe("Terminal font size")]
    // FontSize,
}

#[async_trait]
impl DefaultSettings for CoreSettings {
    async fn defaults() -> Vec<(Self, Value)> {
        vec![]
    }
}
