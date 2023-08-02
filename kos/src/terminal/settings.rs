use crate::imports::*;

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum TerminalSettings {
    Greeting,
    Scrollback,
    FontSize,
}

#[async_trait]
impl DefaultSettings for TerminalSettings {
    async fn defaults() -> Vec<(Self, Value)> {
        vec![]
    }
}
