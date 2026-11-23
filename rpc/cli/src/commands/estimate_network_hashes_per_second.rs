//! `estimate-network-hashes-per-second` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{EstimateNetworkHashesPerSecondRequest, EstimateNetworkHashesPerSecondResponse, RpcHash};
use std::sync::Arc;

/// Estimate the network hashes per second over a window.
#[derive(clap::Args, Debug)]
pub struct EstimateNetworkHashesPerSecond {
    /// Number of blocks in the sampling window.
    #[arg(long)]
    pub window_size: u32,
    /// Hash to start the window from (hex); omit to use the current sink.
    #[arg(long)]
    pub start_hash: Option<RpcHash>,
}

impl RpcCommand for EstimateNetworkHashesPerSecond {
    type Output = EstimateNetworkHashesPerSecondResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = EstimateNetworkHashesPerSecondRequest { window_size: self.window_size, start_hash: self.start_hash };
        Ok(client.estimate_network_hashes_per_second_call(None, request).await?)
    }
}
