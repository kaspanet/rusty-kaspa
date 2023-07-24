// use kaspa_rpc_core::{ConsensusMetrics, ProcessMetrics};

use crate::imports::*;
use kaspa_cli::metrics::MetricsData;
// use kaspa_rpc_core::

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum MetricsOps {
    MetricsData,
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

    // pub async fn post(&self, data: MetricsData) -> Result<()> {
    //     self.target.notify(MetricsOps::MetricsData, data).await?;
    //     Ok(())
    // }
}

// #[async_trait]
// impl MetricsCtl for MetricsIpc {
//     async fn post_data(&self, data : MetricsData) -> IpcResult<()> {
//         self.post(data).await
//     }
// }

#[async_trait]
pub trait MetricsCtl: Send + Sync + 'static {
    async fn post_data(&self, data: MetricsData) -> Result<()>;
}
