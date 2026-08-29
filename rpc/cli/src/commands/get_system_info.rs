//! `get-system-info` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetSystemInfoRequest, GetSystemInfoResponse};
use std::sync::Arc;

/// Get system information about the node host.
#[derive(clap::Args, Debug)]
pub struct GetSystemInfo {}

impl RpcCommand for GetSystemInfo {
    type Output = GetSystemInfoResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_system_info_call(None, GetSystemInfoRequest {}).await?)
    }
}
