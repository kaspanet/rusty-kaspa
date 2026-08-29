//! `get-subnetwork` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetSubnetworkRequest, GetSubnetworkResponse, RpcSubnetworkId};
use std::sync::Arc;

/// Get information about a subnetwork.
#[derive(clap::Args, Debug)]
pub struct GetSubnetwork {
    /// The subnetwork id (hex).
    #[arg(long)]
    pub subnetwork_id: RpcSubnetworkId,
}

impl RpcCommand for GetSubnetwork {
    type Output = GetSubnetworkResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetSubnetworkRequest { subnetwork_id: self.subnetwork_id };
        Ok(client.get_subnetwork_call(None, request).await?)
    }
}
