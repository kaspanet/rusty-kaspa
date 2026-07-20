//! `get-sink` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetSinkRequest, GetSinkResponse};
use std::sync::Arc;

/// Get the hash of the current sink (selected tip) block.
#[derive(clap::Args, Debug)]
pub struct GetSink {}

impl RpcCommand for GetSink {
    type Output = GetSinkResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_sink_call(None, GetSinkRequest {}).await?)
    }
}
