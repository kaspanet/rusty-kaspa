//! `get-block-dag-info` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetBlockDagInfoRequest, GetBlockDagInfoResponse};
use std::sync::Arc;

/// Get general information about the block DAG.
#[derive(clap::Args, Debug)]
pub struct GetBlockDagInfo {}

impl RpcCommand for GetBlockDagInfo {
    type Output = GetBlockDagInfoResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_block_dag_info_call(None, GetBlockDagInfoRequest {}).await?)
    }
}
