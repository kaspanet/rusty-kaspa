use crate::imports::*;
use kaspa_cli_lib::metrics::MetricsSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum MetricsOps {
    MetricsSnapshot,
}

#[derive(Debug, Clone)]
pub struct MetricsIpc {
    #[allow(dead_code)]
    target: IpcTarget,
}

impl MetricsIpc {
    pub fn new(target: IpcTarget) -> MetricsIpc {
        MetricsIpc { target }
    }
}

#[async_trait]
pub trait MetricsCtl: Send + Sync + 'static {
    async fn post_data(&self, data: MetricsSnapshot) -> Result<()>;
}
