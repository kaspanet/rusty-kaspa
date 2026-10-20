//! `get-current-block-color` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetCurrentBlockColorRequest, GetCurrentBlockColorResponse, RpcHash};
use std::sync::Arc;

/// Get whether a block is currently colored blue.
#[derive(clap::Args, Debug)]
pub struct GetCurrentBlockColor {
    /// The hash of the block to query (hex).
    #[arg(long)]
    pub hash: RpcHash,
}

impl RpcCommand for GetCurrentBlockColor {
    type Output = GetCurrentBlockColorResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetCurrentBlockColorRequest { hash: self.hash };
        Ok(client.get_current_block_color_call(None, request).await?)
    }
}
