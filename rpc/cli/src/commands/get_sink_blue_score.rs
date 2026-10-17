//! `get-sink-blue-score` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetSinkBlueScoreRequest, GetSinkBlueScoreResponse};
use std::sync::Arc;

/// Get the blue score of the current sink block.
#[derive(clap::Args, Debug)]
pub struct GetSinkBlueScore {}

impl RpcCommand for GetSinkBlueScore {
    type Output = GetSinkBlueScoreResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_sink_blue_score_call(None, GetSinkBlueScoreRequest {}).await?)
    }
}
