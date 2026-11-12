//! `add-peer` command.
//!
//! Admin / unsafe RPC: only works against a node started with unsafe RPC
//! methods enabled (e.g. an admin / simnet node).

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::RpcContextualPeerAddress;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::model::message::{AddPeerRequest, AddPeerResponse};
use std::sync::Arc;

/// Add a peer to the address manager (requires an admin / unsafe-enabled node).
#[derive(clap::Args, Debug)]
pub struct AddPeer {
    /// Peer address in `ip:port` form (optionally `id@ip:port`).
    #[arg(long = "peer")]
    peer_address: RpcContextualPeerAddress,

    /// Keep the peer permanently across restarts.
    #[arg(long)]
    is_permanent: bool,
}

impl RpcCommand for AddPeer {
    type Output = AddPeerResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = AddPeerRequest::new(self.peer_address, self.is_permanent);
        Ok(client.add_peer_call(None, request).await?)
    }
}
