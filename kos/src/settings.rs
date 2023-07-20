use crate::imports::*;

#[derive(Describe, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum AppSettings {
    #[describe("Welcome message")]
    Greeting,
}

#[async_trait]
impl DefaultSettings for AppSettings {
    async fn defaults() -> Vec<(Self, String)> {
        vec![]
    }
}
