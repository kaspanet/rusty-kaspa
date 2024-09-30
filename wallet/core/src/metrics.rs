//!
//! Primitives for network metrics.
//!

use crate::imports::*;

/// Metrics posted by the wallet subsystem.
/// See [`UtxoProcessor::start_metrics`] to enable metrics processing.
/// This struct contains mempool size that can be used to estimate
/// current network congestion.
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "kebab-case")]
pub enum MetricsUpdate {
    WalletMetrics {
        #[serde(rename = "mempoolSize")]
        mempool_size: u64,
    },
}

/// [`MetricsUpdate`] variant identifier.
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum MetricsUpdateKind {
    WalletMetrics,
}

impl MetricsUpdate {
    pub fn kind(&self) -> MetricsUpdateKind {
        match self {
            MetricsUpdate::WalletMetrics { .. } => MetricsUpdateKind::WalletMetrics,
        }
    }
}
