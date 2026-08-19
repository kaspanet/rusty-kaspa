//! `get-seq-commit-lane-proof` command (KIP-21).

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetSeqCommitLaneProofRequest, GetSeqCommitLaneProofResponse, RpcHash};
use std::sync::Arc;

/// Get a sequence-commitment lane proof for a block.
#[derive(clap::Args, Debug)]
pub struct GetSeqCommitLaneProof {
    /// The block hash to prove against (hex).
    #[arg(long)]
    pub block_hash: RpcHash,
    /// The lane key to prove (hex).
    #[arg(long)]
    pub lane_key: RpcHash,
}

impl RpcCommand for GetSeqCommitLaneProof {
    type Output = GetSeqCommitLaneProofResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetSeqCommitLaneProofRequest { block_hash: self.block_hash, lane_key: self.lane_key };
        Ok(client.get_seq_commit_lane_proof_call(None, request).await?)
    }
}
