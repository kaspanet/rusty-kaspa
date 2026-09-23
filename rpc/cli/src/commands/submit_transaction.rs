//! `submit-transaction` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{RpcTransaction, SubmitTransactionRequest, SubmitTransactionResponse};
use std::sync::Arc;

/// Submit a transaction into the mempool.
#[derive(clap::Args, Debug)]
pub struct SubmitTransaction {
    /// Transaction as JSON: an inline document, `@path` to a file, or `@-` for stdin.
    #[arg(long, value_parser = crate::args::json_value::<RpcTransaction>)]
    transaction: RpcTransaction,

    /// Accept the transaction even if its inputs are not yet known (orphan).
    #[arg(long)]
    allow_orphan: bool,
}

impl RpcCommand for SubmitTransaction {
    type Output = SubmitTransactionResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = SubmitTransactionRequest { transaction: self.transaction.clone(), allow_orphan: self.allow_orphan };
        Ok(client.submit_transaction_call(None, request).await?)
    }
}
