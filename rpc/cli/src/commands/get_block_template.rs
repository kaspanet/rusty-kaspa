//! `get-block-template` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetBlockTemplateRequest, GetBlockTemplateResponse, RpcAddress};
use std::sync::Arc;

/// Request a current block template for mining.
#[derive(clap::Args, Debug)]
pub struct GetBlockTemplate {
    /// Kaspa address the coinbase reward should pay into.
    #[arg(long, value_parser = crate::args::parse_address)]
    pay_address: RpcAddress,

    /// Extra data embedded in the coinbase, interpreted as raw UTF-8 bytes.
    #[arg(long, default_value = "")]
    extra_data: String,
}

impl RpcCommand for GetBlockTemplate {
    type Output = GetBlockTemplateResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request =
            GetBlockTemplateRequest { pay_address: self.pay_address.clone(), extra_data: self.extra_data.as_bytes().to_vec() };
        Ok(client.get_block_template_call(None, request).await?)
    }
}
