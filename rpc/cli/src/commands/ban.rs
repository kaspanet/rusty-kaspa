//! `ban` command.
//!
//! Admin / unsafe RPC: only works against a node started with unsafe RPC
//! methods enabled (e.g. an admin / simnet node).

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::RpcIpAddress;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::model::message::{BanRequest, BanResponse};
use std::sync::Arc;

/// Ban a peer by IP address (requires an admin / unsafe-enabled node).
#[derive(clap::Args, Debug)]
pub struct Ban {
    /// IP address to ban.
    #[arg(long)]
    ip: RpcIpAddress,
}

impl RpcCommand for Ban {
    type Output = BanResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.ban_call(None, BanRequest::new(self.ip)).await?)
    }
}
