//! `get-connected-peer-info` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetConnectedPeerInfoRequest, GetConnectedPeerInfoResponse};
use std::sync::Arc;

/// Get information about currently connected peers.
#[derive(clap::Args, Debug)]
pub struct GetConnectedPeerInfo {}

impl RpcCommand for GetConnectedPeerInfo {
    type Output = GetConnectedPeerInfoResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_connected_peer_info_call(None, GetConnectedPeerInfoRequest {}).await?)
    }
}
