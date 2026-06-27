//! `get-balances-by-addresses` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetBalancesByAddressesRequest, GetBalancesByAddressesResponse, RpcAddress};
use std::sync::Arc;

/// Get balances for the given addresses (node must run with --utxoindex).
#[derive(clap::Args, Debug)]
pub struct GetBalancesByAddresses {
    /// Address to query (bech32). Repeat to query multiple addresses.
    #[arg(long = "address", required = true, value_parser = crate::args::parse_address)]
    addresses: Vec<RpcAddress>,
}

impl RpcCommand for GetBalancesByAddresses {
    type Output = GetBalancesByAddressesResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetBalancesByAddressesRequest { addresses: self.addresses.clone() };
        Ok(client.get_balances_by_addresses_call(None, request).await?)
    }
}
