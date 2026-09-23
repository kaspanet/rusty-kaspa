//! `get-block` command.
//!
//! Template for a command with a `FromStr` request field: `hash: RpcHash` is
//! declared directly and clap derives its parser from `FromStr`. `run` moves
//! the already-typed fields into the request and returns the typed response.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetBlockRequest, GetBlockResponse, RpcHash};
use std::sync::Arc;

/// Get a block by hash.
#[derive(clap::Args, Debug)]
pub struct GetBlock {
    /// The hash of the requested block (hex).
    #[arg(long)]
    pub hash: RpcHash,
    /// Include transaction data in the response.
    #[arg(long)]
    pub include_transactions: bool,
}

impl RpcCommand for GetBlock {
    type Output = GetBlockResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetBlockRequest { hash: self.hash, include_transactions: self.include_transactions };
        Ok(client.get_block_call(None, request).await?)
    }
}
