//! `get-balance-by-address` command.
//!
//! Template for a command with a request field whose type lacks `FromStr`:
//! `address: RpcAddress` uses an explicit `value_parser = crate::args::parse_address`
//! so clap parses the bech32 string per occurrence. `run` moves the typed field
//! into the request and returns the typed response.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetBalanceByAddressRequest, GetBalanceByAddressResponse, RpcAddress};
use std::sync::Arc;

/// Get the balance of an address (node must run with --utxoindex).
#[derive(clap::Args, Debug)]
pub struct GetBalanceByAddress {
    /// Address to query (bech32).
    #[arg(long = "address", value_parser = crate::args::parse_address)]
    address: RpcAddress,
}

impl RpcCommand for GetBalanceByAddress {
    type Output = GetBalanceByAddressResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetBalanceByAddressRequest { address: self.address.clone() };
        Ok(client.get_balance_by_address_call(None, request).await?)
    }
}
