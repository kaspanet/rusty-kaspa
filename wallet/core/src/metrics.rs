use crate::imports::*;
// use kaspa_metrics_core::MetricsSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "kebab-case")]
pub enum MetricsUpdate {
    WalletMetrics {
        #[serde(rename = "mempoolSize")]
        mempool_size: u64,
        #[serde(rename = "nodePeers")]
        node_peers: u32,
        #[serde(rename = "networkTPS")]
        network_tps: f64,
    },
    // NodeMetrics {
    //     snapshot : Box<MetricsSnapshot>
    // }
}

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum MetricsUpdateKind {
    WalletMetrics,
    // NodeMetrics
}

impl MetricsUpdate {
    pub fn kind(&self) -> MetricsUpdateKind {
        match self {
            MetricsUpdate::WalletMetrics { .. } => MetricsUpdateKind::WalletMetrics,
            // MetricsUpdate::NodeMetrics { .. } => MetricsUpdateKind::NodeMetrics
        }
    }
}

// impl MetricsUpdate {
//     pub fn wallet_metrics(mempool_size: u64, peers: usize) -> Self {
//         MetricsUpdate::WalletMetrics { mempool_size, peers }
//     }

//     pub fn node_metrics(snapshot: MetricsSnapshot) -> Self {
//         MetricsUpdate::NodeMetrics(Box::new(snapshot))
//     }
// }
