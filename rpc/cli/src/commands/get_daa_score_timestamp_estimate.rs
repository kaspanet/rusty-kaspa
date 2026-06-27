//! `get-daa-score-timestamp-estimate` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetDaaScoreTimestampEstimateRequest, GetDaaScoreTimestampEstimateResponse};
use std::sync::Arc;

/// Estimate the timestamps at which the given DAA scores were reached.
#[derive(clap::Args, Debug)]
pub struct GetDaaScoreTimestampEstimate {
    /// DAA score to estimate a timestamp for (repeatable).
    #[arg(long = "daa-score")]
    pub daa_scores: Vec<u64>,
}

impl RpcCommand for GetDaaScoreTimestampEstimate {
    type Output = GetDaaScoreTimestampEstimateResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetDaaScoreTimestampEstimateRequest { daa_scores: self.daa_scores.clone() };
        Ok(client.get_daa_score_timestamp_estimate_call(None, request).await?)
    }
}
