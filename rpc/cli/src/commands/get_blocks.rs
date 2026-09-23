//! `get-blocks` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetBlocksRequest, GetBlocksResponse, RpcHash};
use std::sync::Arc;

/// Get blocks starting from a low hash.
#[derive(clap::Args, Debug)]
pub struct GetBlocks {
    /// Lowest block hash to start from (hex); omit to start from the pruning point.
    #[arg(long)]
    pub low_hash: Option<RpcHash>,
    /// Include full block data in the response.
    #[arg(long)]
    pub include_blocks: bool,
    /// Include transaction data in the response.
    #[arg(long)]
    pub include_transactions: bool,
}

impl RpcCommand for GetBlocks {
    type Output = GetBlocksResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetBlocksRequest {
            low_hash: self.low_hash,
            include_blocks: self.include_blocks,
            include_transactions: self.include_transactions,
        };
        Ok(client.get_blocks_call(None, request).await?)
    }
}
