//! `get-fee-estimate-experimental` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetFeeEstimateExperimentalRequest, GetFeeEstimateExperimentalResponse};
use std::sync::Arc;

/// Get an experimental fee-rate estimate, optionally with verbose details.
#[derive(clap::Args, Debug)]
pub struct GetFeeEstimateExperimental {
    /// Include verbose estimation details in the response.
    /// (Named `--verbose-data` to avoid colliding with the global `--verbose`.)
    #[arg(long = "verbose-data")]
    pub verbose_data: bool,
}

impl RpcCommand for GetFeeEstimateExperimental {
    type Output = GetFeeEstimateExperimentalResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetFeeEstimateExperimentalRequest { verbose: self.verbose_data };
        Ok(client.get_fee_estimate_experimental_call(None, request).await?)
    }
}
