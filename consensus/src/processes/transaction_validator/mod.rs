pub mod errors;
mod tx_validation_in_isolation;
pub use tx_validation_in_isolation::*;

#[derive(Clone, Copy)]
pub struct TransactionValidator {
    max_tx_inputs: usize,
    max_tx_outputs: usize,
    max_signature_script_len: usize,
    max_script_public_key_len: usize,
}

impl TransactionValidator {
    pub fn new(
        max_tx_inputs: usize, max_tx_outputs: usize, max_signature_script_len: usize, max_script_public_key_len: usize,
    ) -> Self {
        Self { max_tx_inputs, max_tx_outputs, max_signature_script_len, max_script_public_key_len }
    }
}
