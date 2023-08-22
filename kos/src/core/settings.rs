use crate::imports::*;

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum CoreSettings {
    Metrics,
}

#[async_trait]
impl DefaultSettings for CoreSettings {
    async fn defaults() -> Vec<(Self, Value)> {
        vec![]
    }
}
