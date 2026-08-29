//! `get-server-info` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetServerInfoRequest, GetServerInfoResponse};
use std::sync::Arc;

/// Get server information about the node.
#[derive(clap::Args, Debug)]
pub struct GetServerInfo {}

impl RpcCommand for GetServerInfo {
    type Output = GetServerInfoResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_server_info_call(None, GetServerInfoRequest {}).await?)
    }
}
