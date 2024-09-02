pub mod errors;
pub mod transaction_validator_populated;
mod tx_validation_in_isolation;
pub mod tx_validation_not_utxo_related;
use std::sync::Arc;

use crate::model::stores::ghostdag;

use kaspa_txscript::{
    caches::{Cache, TxScriptCacheCounters},
    SigCacheKey,
};

use kaspa_consensus_core::mass::MassCalculator;

#[derive(Clone)]
pub struct TransactionValidator {
    max_tx_inputs: usize,
    max_tx_outputs: usize,
    max_signature_script_len: usize,
    max_script_public_key_len: usize,
    ghostdag_k: ghostdag::KType,
    coinbase_payload_script_public_key_max_len: u8,
    coinbase_maturity: u64,
    sig_cache: Cache<SigCacheKey, bool>,

    pub(crate) mass_calculator: MassCalculator,

    /// Storage mass hardfork DAA score
    storage_mass_activation_daa_score: u64,
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
        counters: Arc<TxScriptCacheCounters>,
        mass_calculator: MassCalculator,
        storage_mass_activation_daa_score: u64,
    ) -> Self {
        Self {
            max_tx_inputs,
            max_tx_outputs,
            max_signature_script_len,
            max_script_public_key_len,
            ghostdag_k,
            coinbase_payload_script_public_key_max_len,
            coinbase_maturity,
            sig_cache: Cache::with_counters(10_000, counters),
            mass_calculator,
            storage_mass_activation_daa_score,
        }
    }

    pub fn new_for_tests(
        max_tx_inputs: usize,
        max_tx_outputs: usize,
        max_signature_script_len: usize,
        max_script_public_key_len: usize,
        ghostdag_k: ghostdag::KType,
        coinbase_payload_script_public_key_max_len: u8,
        coinbase_maturity: u64,
        counters: Arc<TxScriptCacheCounters>,
    ) -> Self {
        Self {
            max_tx_inputs,
            max_tx_outputs,
            max_signature_script_len,
            max_script_public_key_len,
            ghostdag_k,
            coinbase_payload_script_public_key_max_len,
            coinbase_maturity,
            sig_cache: Cache::with_counters(10_000, counters),
            mass_calculator: MassCalculator::new(0, 0, 0, 0),
            storage_mass_activation_daa_score: u64::MAX,
        }
    }
}
