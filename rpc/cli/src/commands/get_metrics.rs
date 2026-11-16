//! `get-metrics` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetMetricsRequest, GetMetricsResponse};
use std::sync::Arc;

/// Get node metrics. With no flags, all metric sections are requested;
/// passing any flag selects only the requested sections.
#[derive(clap::Args, Debug)]
pub struct GetMetrics {
    /// Include process metrics.
    #[arg(long)]
    pub process: bool,
    /// Include connection metrics.
    #[arg(long)]
    pub connections: bool,
    /// Include bandwidth metrics.
    #[arg(long)]
    pub bandwidth: bool,
    /// Include consensus metrics.
    #[arg(long)]
    pub consensus: bool,
    /// Include storage metrics.
    #[arg(long)]
    pub storage: bool,
    /// Include custom metrics.
    #[arg(long)]
    pub custom: bool,
}

impl RpcCommand for GetMetrics {
    type Output = GetMetricsResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        // When no section flag is provided, request all sections (default ALL true).
        let all = !(self.process || self.connections || self.bandwidth || self.consensus || self.storage || self.custom);
        let request = GetMetricsRequest {
            process_metrics: self.process || all,
            connection_metrics: self.connections || all,
            bandwidth_metrics: self.bandwidth || all,
            consensus_metrics: self.consensus || all,
            storage_metrics: self.storage || all,
            custom_metrics: self.custom || all,
        };
        Ok(client.get_metrics_call(None, request).await?)
    }
}
