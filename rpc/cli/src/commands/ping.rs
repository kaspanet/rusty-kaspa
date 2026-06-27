//! `ping` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{PingRequest, PingResponse};
use std::sync::Arc;

/// Ping the node to check connectivity.
#[derive(clap::Args, Debug)]
pub struct Ping {}

impl RpcCommand for Ping {
    type Output = PingResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.ping_call(None, PingRequest {}).await?)
    }
}
