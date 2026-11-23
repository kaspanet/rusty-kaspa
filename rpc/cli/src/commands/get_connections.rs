//! `get-connections` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetConnectionsRequest, GetConnectionsResponse};
use std::sync::Arc;

/// Get active connection counts (optionally with profiling data).
#[derive(clap::Args, Debug)]
pub struct GetConnections {
    /// Include per-connection profiling data in the response.
    #[arg(long)]
    pub profile_data: bool,
}

impl RpcCommand for GetConnections {
    type Output = GetConnectionsResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetConnectionsRequest { include_profile_data: self.profile_data };
        Ok(client.get_connections_call(None, request).await?)
    }
}
