pub mod errors;
pub mod transaction_validator_populated;
mod tx_validation_in_isolation;
pub mod tx_validation_not_utxo_related;
use crate::model::stores::{database::prelude::Cache, ghostdag};

pub use tx_validation_in_isolation::*;

// TODO: Move it to the script engine once it's ready
#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) struct SigCacheKey {
    signature: secp256k1::schnorr::Signature,
    pub_key: secp256k1::XOnlyPublicKey,
    message: secp256k1::Message,
}

#[derive(Clone)]
pub struct TransactionValidator {
    max_tx_inputs: usize,
    max_tx_outputs: usize,
    max_signature_script_len: usize,
    max_script_public_key_len: usize,
    ghostdag_k: ghostdag::KType,
    coinbase_payload_script_public_key_max_len: u8,
    coinbase_maturity: u64,
    sig_cache: Cache<SigCacheKey, bool>, // TODO: Move sig_cache to the script engine once it's ready
}

impl TransactionValidator {
    pub fn new(
        max_tx_inputs: usize,
        max_tx_outputs: usize,
        max_signature_script_len: usize,
        max_script_public_key_len: usize,
        ghostdag_k: ghostdag::KType,
        coinbase_payload_script_public_key_max_len: u8,
        coinbase_maturity: u64,
    ) -> Self {
        Self {
            max_tx_inputs,
            max_tx_outputs,
            max_signature_script_len,
            max_script_public_key_len,
            ghostdag_k,
            coinbase_payload_script_public_key_max_len,
            coinbase_maturity,
            sig_cache: Cache::new(10_000),
        }
    }
}
