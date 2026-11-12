//! `get-current-network` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetCurrentNetworkRequest, GetCurrentNetworkResponse};
use std::sync::Arc;

/// Get the current network (mainnet, testnet, etc.).
#[derive(clap::Args, Debug)]
pub struct GetCurrentNetwork {}

impl RpcCommand for GetCurrentNetwork {
    type Output = GetCurrentNetworkResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_current_network_call(None, GetCurrentNetworkRequest {}).await?)
    }
}
