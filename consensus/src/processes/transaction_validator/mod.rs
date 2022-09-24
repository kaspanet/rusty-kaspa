pub mod errors;
pub mod transaction_validator_populated;
mod tx_validation_in_isolation;
pub mod tx_validation_not_utxo_related;
use std::sync::Arc;

pub use tx_validation_in_isolation::*;

use crate::model::stores::headers::HeaderStoreReader;

#[derive(Clone)]
pub struct TransactionValidator<T: HeaderStoreReader> {
    max_tx_inputs: usize,
    max_tx_outputs: usize,
    max_signature_script_len: usize,
    max_script_public_key_len: usize,
    coinbase_maturity: u64,

    headers_store: Arc<T>,
}

impl<T: HeaderStoreReader> TransactionValidator<T> {
    pub fn new(
        max_tx_inputs: usize,
        max_tx_outputs: usize,
        max_signature_script_len: usize,
        max_script_public_key_len: usize,
        coinbase_maturity: u64,
        headers_store: Arc<T>,
    ) -> Self {
        Self { max_tx_inputs, max_tx_outputs, max_signature_script_len, max_script_public_key_len, coinbase_maturity, headers_store }
    }
}
