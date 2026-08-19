//! `get-headers` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetHeadersRequest, GetHeadersResponse, RpcHash};
use std::sync::Arc;

/// Get block headers starting from a hash.
#[derive(clap::Args, Debug)]
pub struct GetHeaders {
    /// Hash to start from (hex).
    #[arg(long)]
    pub start_hash: RpcHash,
    /// Maximum number of headers to return.
    #[arg(long)]
    pub limit: u64,
    /// Return headers in ascending order.
    #[arg(long)]
    pub is_ascending: bool,
}

impl RpcCommand for GetHeaders {
    type Output = GetHeadersResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = GetHeadersRequest { start_hash: self.start_hash, limit: self.limit, is_ascending: self.is_ascending };
        Ok(client.get_headers_call(None, request).await?)
    }
}
