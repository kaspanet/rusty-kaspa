use kaspa_rpc_core::{ConsensusMetrics, ProcessMetrics};

use crate::imports::*;
// use kaspa_rpc_core::

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum MetricsOps {
    MetricsData,
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum MetricsData {
    Tps(u64),
    ConsensusMetrics(ConsensusMetrics),
    ProcessMetrics(ProcessMetrics),
}

#[derive(Debug, Clone)]
pub struct MetricsIpc {
    target: IpcTarget,
}

impl MetricsIpc {
    pub fn new(target: IpcTarget) -> MetricsIpc {
        MetricsIpc { target }
    }

    pub async fn post_data(&self, data: MetricsData) -> Result<()> {
        self.target.notify(MetricsOps::MetricsData, data).await?;
        Ok(())
    }
}
