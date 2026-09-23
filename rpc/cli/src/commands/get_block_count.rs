//! `get-block-count` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetBlockCountRequest, GetBlockCountResponse};
use std::sync::Arc;

/// Get the current block and header counts.
#[derive(clap::Args, Debug)]
pub struct GetBlockCount {}

impl RpcCommand for GetBlockCount {
    type Output = GetBlockCountResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_block_count_call(None, GetBlockCountRequest {}).await?)
    }
}
