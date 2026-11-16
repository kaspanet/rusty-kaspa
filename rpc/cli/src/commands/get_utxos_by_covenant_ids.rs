//! `get-utxos-by-covenant-ids` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{
    GetUtxosByCovenantIdsRequest, GetUtxosByCovenantIdsResponse, RpcGetUtxosByCovenantIdsCursor, RpcHash, RpcScriptPublicKey,
};
use std::sync::Arc;

/// Get paginated UTXOs for covenant IDs (node must run with --utxoindex --covenant-index).
#[derive(clap::Args, Debug)]
pub struct GetUtxosByCovenantIds {
    /// Covenant ID to query. Repeat to query multiple IDs.
    #[arg(long = "covenant-id", required = true)]
    covenant_ids: Vec<RpcHash>,

    /// Script public-key hex filter. Repeat to include multiple scripts; omit to include all scripts.
    #[arg(long = "script-public-key")]
    script_public_keys: Vec<RpcScriptPublicKey>,

    /// Inclusive lower DAA-score bound.
    #[arg(long)]
    from_daa_score: Option<u64>,

    /// Inclusive upper DAA-score bound.
    #[arg(long)]
    to_daa_score: Option<u64>,

    /// Complete continuation cursor from the previous response, encoded as JSON.
    #[arg(long, value_parser = crate::args::json_value::<RpcGetUtxosByCovenantIdsCursor>)]
    cursor: Option<RpcGetUtxosByCovenantIdsCursor>,

    /// Weighted database-operation budget; defaults to 1000 and values above 5000 require --unsaferpc on the node.
    #[arg(long)]
    budget: Option<usize>,
}

impl RpcCommand for GetUtxosByCovenantIds {
    type Output = GetUtxosByCovenantIdsResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetUtxosByCovenantIdsRequest::new(
            self.covenant_ids.clone(),
            self.script_public_keys.clone(),
            self.from_daa_score,
            self.to_daa_score,
            self.cursor.clone(),
            self.budget,
        );
        Ok(client.get_utxos_by_covenant_ids_call(None, request).await?)
    }
}
