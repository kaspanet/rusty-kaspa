//! `get-mempool-entries` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetMempoolEntriesRequest, GetMempoolEntriesResponse};
use std::sync::Arc;

/// Get all mempool entries.
#[derive(clap::Args, Debug)]
pub struct GetMempoolEntries {
    /// Also search the orphan pool.
    #[arg(long)]
    include_orphan_pool: bool,

    /// Filter out entries from the transaction pool.
    #[arg(long)]
    filter_transaction_pool: bool,
}

impl RpcCommand for GetMempoolEntries {
    type Output = GetMempoolEntriesResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetMempoolEntriesRequest {
            include_orphan_pool: self.include_orphan_pool,
            filter_transaction_pool: self.filter_transaction_pool,
        };
        Ok(client.get_mempool_entries_call(None, request).await?)
    }
}
