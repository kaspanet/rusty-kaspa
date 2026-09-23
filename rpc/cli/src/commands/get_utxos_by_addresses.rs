//! `get-utxos-by-addresses` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetUtxosByAddressesRequest, GetUtxosByAddressesResponse, RpcAddress};
use std::sync::Arc;

/// Get UTXOs for the given addresses (node must run with --utxoindex).
#[derive(clap::Args, Debug)]
pub struct GetUtxosByAddresses {
    /// Address to query (bech32). Repeat to query multiple addresses.
    #[arg(long = "address", required = true, value_parser = crate::args::parse_address)]
    addresses: Vec<RpcAddress>,
}

impl RpcCommand for GetUtxosByAddresses {
    type Output = GetUtxosByAddressesResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetUtxosByAddressesRequest { addresses: self.addresses.clone() };
        Ok(client.get_utxos_by_addresses_call(None, request).await?)
    }
}
