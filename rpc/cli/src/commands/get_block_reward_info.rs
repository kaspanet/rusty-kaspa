//! `get-block-reward-info` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetBlockRewardInfoRequest, GetBlockRewardInfoResponse, RpcHash};
use std::sync::Arc;

/// Get block reward information for a block.
#[derive(clap::Args, Debug)]
pub struct GetBlockRewardInfo {
    /// The hash of the block to query (hex).
    #[arg(long)]
    pub hash: RpcHash,
}

impl RpcCommand for GetBlockRewardInfo {
    type Output = GetBlockRewardInfoResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetBlockRewardInfoRequest { hash: self.hash };
        Ok(client.get_block_reward_info_call(None, request).await?)
    }
}
