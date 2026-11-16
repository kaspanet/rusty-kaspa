//! `get-mempool-entries-by-addresses` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetMempoolEntriesByAddressesRequest, GetMempoolEntriesByAddressesResponse, RpcAddress};
use std::sync::Arc;

/// Get mempool entries for the given addresses.
#[derive(clap::Args, Debug)]
pub struct GetMempoolEntriesByAddresses {
    /// Address to query (bech32). Repeat to query multiple addresses.
    #[arg(long = "address", required = true, value_parser = crate::args::parse_address)]
    addresses: Vec<RpcAddress>,

    /// Also search the orphan pool.
    #[arg(long)]
    include_orphan_pool: bool,

    /// Filter out entries from the transaction pool.
    #[arg(long)]
    filter_transaction_pool: bool,
}

impl RpcCommand for GetMempoolEntriesByAddresses {
    type Output = GetMempoolEntriesByAddressesResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetMempoolEntriesByAddressesRequest {
            addresses: self.addresses.clone(),
            include_orphan_pool: self.include_orphan_pool,
            filter_transaction_pool: self.filter_transaction_pool,
        };
        Ok(client.get_mempool_entries_by_addresses_call(None, request).await?)
    }
}
