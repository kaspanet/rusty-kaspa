//! `unban` command.
//!
//! Admin / unsafe RPC: only works against a node started with unsafe RPC
//! methods enabled (e.g. an admin / simnet node).

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::RpcIpAddress;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::model::message::{UnbanRequest, UnbanResponse};
use std::sync::Arc;

/// Lift a ban on a peer IP address (requires an admin / unsafe-enabled node).
#[derive(clap::Args, Debug)]
pub struct Unban {
    /// IP address to unban.
    #[arg(long)]
    ip: RpcIpAddress,
}

impl RpcCommand for Unban {
    type Output = UnbanResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.unban_call(None, UnbanRequest::new(self.ip)).await?)
    }
}
