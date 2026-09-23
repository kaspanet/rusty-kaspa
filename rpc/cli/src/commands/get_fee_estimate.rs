//! `get-fee-estimate` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetFeeEstimateRequest, GetFeeEstimateResponse};
use std::sync::Arc;

/// Get a fee-rate estimate for transactions.
#[derive(clap::Args, Debug)]
pub struct GetFeeEstimate {}

impl RpcCommand for GetFeeEstimate {
    type Output = GetFeeEstimateResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_fee_estimate_call(None, GetFeeEstimateRequest {}).await?)
    }
}
