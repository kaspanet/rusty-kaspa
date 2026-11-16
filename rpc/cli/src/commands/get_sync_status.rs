//! `get-sync-status` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetSyncStatusRequest, GetSyncStatusResponse};
use std::sync::Arc;

/// Get whether the node is synced.
#[derive(clap::Args, Debug)]
pub struct GetSyncStatus {}

impl RpcCommand for GetSyncStatus {
    type Output = GetSyncStatusResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_sync_status_call(None, GetSyncStatusRequest {}).await?)
    }
}
