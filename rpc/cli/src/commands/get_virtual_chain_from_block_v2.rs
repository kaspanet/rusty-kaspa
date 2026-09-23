//! `get-virtual-chain-from-block-v2` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::model::verbosity::RpcDataVerbosityLevel;
use kaspa_rpc_core::{GetVirtualChainFromBlockV2Request, GetVirtualChainFromBlockV2Response, RpcHash};
use std::sync::Arc;

/// Get the virtual selected-parent chain from a starting block (v2).
#[derive(clap::Args, Debug)]
pub struct GetVirtualChainFromBlockV2 {
    /// Hash to start from (hex).
    #[arg(long)]
    pub start_hash: RpcHash,
    /// Data verbosity level: `none`/`low`/`high`/`full` or `0`..`3`.
    #[arg(long, value_parser = parse_verbosity)]
    pub verbosity: Option<RpcDataVerbosityLevel>,
    /// Only return chain blocks with at least this many confirmations.
    #[arg(long)]
    pub min_confirmation_count: Option<u64>,
}

fn parse_verbosity(s: &str) -> std::result::Result<RpcDataVerbosityLevel, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "none" | "0" => Ok(RpcDataVerbosityLevel::None),
        "low" | "1" => Ok(RpcDataVerbosityLevel::Low),
        "high" | "2" => Ok(RpcDataVerbosityLevel::High),
        "full" | "3" => Ok(RpcDataVerbosityLevel::Full),
        other => Err(format!("invalid verbosity level: {other} (expected none|low|high|full or 0..3)")),
    }
}

impl RpcCommand for GetVirtualChainFromBlockV2 {
    type Output = GetVirtualChainFromBlockV2Response;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetVirtualChainFromBlockV2Request {
            start_hash: self.start_hash,
            data_verbosity_level: self.verbosity,
            min_confirmation_count: self.min_confirmation_count,
        };
        Ok(client.get_virtual_chain_from_block_v2_call(None, request).await?)
    }
}
