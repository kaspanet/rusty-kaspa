//! `get-peer-addresses` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPeerAddressesRequest, GetPeerAddressesResponse};
use std::sync::Arc;

/// Get known peer addresses.
#[derive(clap::Args, Debug)]
pub struct GetPeerAddresses {}

impl RpcCommand for GetPeerAddresses {
    type Output = GetPeerAddressesResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await?)
    }
}
