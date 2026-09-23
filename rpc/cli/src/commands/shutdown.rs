//! `shutdown` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{ShutdownRequest, ShutdownResponse};
use std::sync::Arc;

/// Request the node to shut down.
#[derive(clap::Args, Debug)]
pub struct Shutdown {}

impl RpcCommand for Shutdown {
    type Output = ShutdownResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.shutdown_call(None, ShutdownRequest {}).await?)
    }
}
