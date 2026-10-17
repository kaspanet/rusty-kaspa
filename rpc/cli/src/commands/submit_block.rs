//! `submit-block` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{RpcRawBlock, SubmitBlockRequest, SubmitBlockResponse};
use std::sync::Arc;

/// Submit a solved block into the DAG.
#[derive(clap::Args, Debug)]
pub struct SubmitBlock {
    /// Block as JSON: an inline document, `@path` to a file, or `@-` for stdin.
    #[arg(long, value_parser = crate::args::json_value::<RpcRawBlock>)]
    block: RpcRawBlock,

    /// Permit blocks whose DAA score has not yet been validated.
    #[arg(long)]
    allow_non_daa_blocks: bool,
}

impl RpcCommand for SubmitBlock {
    type Output = SubmitBlockResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = SubmitBlockRequest { block: self.block.clone(), allow_non_daa_blocks: self.allow_non_daa_blocks };
        Ok(client.submit_block_call(None, request).await?)
    }
}
