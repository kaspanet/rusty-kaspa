//! `get-info` command.
//!
//! THIS MODULE IS THE TEMPLATE for every RPC method command. To add a new
//! method, clone this file and:
//!   1. Rename the struct (PascalCase of the method, e.g. `GetBlock`).
//!   2. Add `#[arg(long)]` fields for each request parameter (none here).
//!      A field whose type implements `FromStr` (e.g. `RpcHash`) is declared
//!      directly (`#[arg(long)] pub hash: RpcHash,`) and clap derives its
//!      parser. A field whose type lacks `FromStr` uses an explicit parser:
//!      `#[arg(long, value_parser = crate::args::parse_address)] pub address: RpcAddress,`.
//!   3. `impl RpcCommand`: set `type Output = <Method>Response` and in `run`
//!      build the matching `<Method>Request { .. }` by moving the typed fields,
//!      call `client.<method>_call(None, request)`, and return the response.
//!   4. Register the struct as a variant in `crate::cli::Commands` and add its
//!      name to the dispatch macro in `crate::dispatch`.
//!
//! The `_call(None, request)` form is used uniformly so the same shape works
//! for methods with and without parameters.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetInfoRequest, GetInfoResponse};
use std::sync::Arc;

/// Get general information about the node.
#[derive(clap::Args, Debug)]
pub struct GetInfo {}

impl RpcCommand for GetInfo {
    type Output = GetInfoResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        Ok(client.get_info_call(None, GetInfoRequest {}).await?)
    }
}
