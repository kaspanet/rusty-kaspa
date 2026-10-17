//! `resolve-finality-conflict` command.
//!
//! Admin / unsafe RPC: only works against a node started with unsafe RPC
//! methods enabled (e.g. an admin / simnet node).

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::RpcHash;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::model::message::{ResolveFinalityConflictRequest, ResolveFinalityConflictResponse};
use std::sync::Arc;

/// Resolve a finality conflict at the given block (requires an admin / unsafe-enabled node).
#[derive(clap::Args, Debug)]
pub struct ResolveFinalityConflict {
    /// Hash of the finality block to keep.
    #[arg(long)]
    finality_block_hash: RpcHash,
}

impl RpcCommand for ResolveFinalityConflict {
    type Output = ResolveFinalityConflictResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = ResolveFinalityConflictRequest::new(self.finality_block_hash);
        Ok(client.resolve_finality_conflict_call(None, request).await?)
    }
}
