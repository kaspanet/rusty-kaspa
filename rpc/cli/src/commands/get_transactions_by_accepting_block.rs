//! `get-transactions-by-accepting-block` command.

use crate::commands::RpcCommand;
use crate::error::{CliError, Result};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{
    GetTransactionsByAcceptingBlockRequest, GetTransactionsByAcceptingBlockResponse, RpcAcceptingBlock, RpcHash, RpcTransactionId,
};
use std::sync::Arc;

/// Get fully populated transactions accepted by a block selected by hash or DAA score.
#[derive(clap::Args, Debug)]
pub struct GetTransactionsByAcceptingBlock {
    /// Hash of the accepting block.
    #[arg(long, required_unless_present = "accepting_block_daa_score", conflicts_with = "accepting_block_daa_score")]
    accepting_block_hash: Option<RpcHash>,

    /// Exact DAA score of the accepting block in the current selected-parent chain.
    #[arg(long, required_unless_present = "accepting_block_hash", conflicts_with = "accepting_block_hash")]
    accepting_block_daa_score: Option<u64>,

    /// Transaction ID accepted by the selected block. Repeat for multiple transactions.
    #[arg(long = "transaction-id", required = true)]
    transaction_ids: Vec<RpcTransactionId>,
}

impl RpcCommand for GetTransactionsByAcceptingBlock {
    type Output = GetTransactionsByAcceptingBlockResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let accepting_block = match (self.accepting_block_hash, self.accepting_block_daa_score) {
            (Some(hash), None) => RpcAcceptingBlock::Hash(hash),
            (None, Some(daa_score)) => RpcAcceptingBlock::DaaScore(daa_score),
            _ => {
                return Err(CliError::Usage(
                    "provide exactly one of --accepting-block-hash or --accepting-block-daa-score".to_string(),
                ));
            }
        };
        let request = GetTransactionsByAcceptingBlockRequest::new(accepting_block, self.transaction_ids.clone());
        Ok(client.get_transactions_by_accepting_block_call(None, request).await?)
    }
}
