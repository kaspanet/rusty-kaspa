//! `get-mempool-entry` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetMempoolEntryRequest, GetMempoolEntryResponse, RpcTransactionId};
use std::sync::Arc;

/// Get a single mempool entry by transaction id.
#[derive(clap::Args, Debug)]
pub struct GetMempoolEntry {
    /// Transaction id (hex).
    #[arg(long = "tx-id")]
    transaction_id: RpcTransactionId,

    /// Also search the orphan pool.
    #[arg(long)]
    include_orphan_pool: bool,

    /// Filter out entries from the transaction pool.
    #[arg(long)]
    filter_transaction_pool: bool,
}

impl RpcCommand for GetMempoolEntry {
    type Output = GetMempoolEntryResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetMempoolEntryRequest {
            transaction_id: self.transaction_id,
            include_orphan_pool: self.include_orphan_pool,
            filter_transaction_pool: self.filter_transaction_pool,
        };
        Ok(client.get_mempool_entry_call(None, request).await?)
    }
}
