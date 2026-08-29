//! `get-coin-supply` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetCoinSupplyRequest, GetCoinSupplyResponse};
use std::sync::Arc;

/// Get the current circulating and max coin supply.
#[derive(clap::Args, Debug)]
pub struct GetCoinSupply {}

impl RpcCommand for GetCoinSupply {
    type Output = GetCoinSupplyResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_coin_supply_call(None, GetCoinSupplyRequest {}).await?)
    }
}
