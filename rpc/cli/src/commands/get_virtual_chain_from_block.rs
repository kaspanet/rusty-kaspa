//! `get-virtual-chain-from-block` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetVirtualChainFromBlockRequest, GetVirtualChainFromBlockResponse, RpcHash};
use std::sync::Arc;

/// Get the virtual selected-parent chain from a starting block.
#[derive(clap::Args, Debug)]
pub struct GetVirtualChainFromBlock {
    /// Hash to start from (hex).
    #[arg(long)]
    pub start_hash: RpcHash,
    /// Include accepted transaction ids in the response.
    #[arg(long)]
    pub include_accepted_transaction_ids: bool,
    /// Only return chain blocks with at least this many confirmations.
    #[arg(long)]
    pub min_confirmation_count: Option<u64>,
}

impl RpcCommand for GetVirtualChainFromBlock {
    type Output = GetVirtualChainFromBlockResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetVirtualChainFromBlockRequest {
            start_hash: self.start_hash,
            include_accepted_transaction_ids: self.include_accepted_transaction_ids,
            min_confirmation_count: self.min_confirmation_count,
        };
        Ok(client.get_virtual_chain_from_block_call(None, request).await?)
    }
}
