//! `submit-transaction-replacement` command.

use crate::commands::RpcCommand;
use crate::error::Result;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{RpcTransaction, SubmitTransactionReplacementRequest, SubmitTransactionReplacementResponse};
use std::sync::Arc;

/// Submit a replacement transaction, evicting a conflicting mempool entry.
#[derive(clap::Args, Debug)]
pub struct SubmitTransactionReplacement {
    /// Transaction as JSON: an inline document, `@path` to a file, or `@-` for stdin.
    #[arg(long, value_parser = crate::args::json_value::<RpcTransaction>)]
    transaction: RpcTransaction,
}

impl RpcCommand for SubmitTransactionReplacement {
    type Output = SubmitTransactionReplacementResponse;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let request = SubmitTransactionReplacementRequest { transaction: self.transaction.clone() };
        Ok(client.submit_transaction_replacement_call(None, request).await?)
    }
}
