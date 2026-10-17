//! `get-utxo-return-address` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetUtxoReturnAddressRequest, GetUtxoReturnAddressResponse, RpcHash};
use std::sync::Arc;

/// Get the return address for the input of a previously accepted transaction.
#[derive(clap::Args, Debug)]
pub struct GetUtxoReturnAddress {
    /// Transaction id (hex).
    #[arg(long = "txid")]
    txid: RpcHash,

    /// DAA score of the block that accepted the transaction.
    #[arg(long = "accepting-block-daa-score")]
    accepting_block_daa_score: u64,
}

impl RpcCommand for GetUtxoReturnAddress {
    type Output = GetUtxoReturnAddressResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetUtxoReturnAddressRequest { txid: self.txid, accepting_block_daa_score: self.accepting_block_daa_score };
        Ok(client.get_utxo_return_address_call(None, request).await?)
    }
}
