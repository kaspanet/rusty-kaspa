pub mod errors;
pub mod tx_validation_in_header_context;
pub mod tx_validation_in_isolation;
pub mod tx_validation_in_utxo_context;
use std::sync::Arc;

use kaspa_txscript::{
    caches::{Cache, TxScriptCacheCounters},
    SigCacheKey,
};

use kaspa_consensus_core::{
    config::params::{ForkActivation, ForkedParam},
    mass::MassCalculator,
    KType,
};

#[derive(Clone)]
pub struct TransactionValidator {
    max_tx_inputs: ForkedParam<usize>,
    max_tx_outputs: ForkedParam<usize>,
    max_signature_script_len: ForkedParam<usize>,
    max_script_public_key_len: ForkedParam<usize>,
    coinbase_payload_script_public_key_max_len: u8,
    coinbase_maturity: ForkedParam<u64>,
    ghostdag_k: KType,
    sig_cache: Cache<SigCacheKey, bool>,

    pub(crate) mass_calculator: MassCalculator,

    /// Crescendo hardfork activation score. Activates KIPs 9, 10, 14
    crescendo_activation: ForkActivation,
}

impl TransactionValidator {
    pub fn new(
        max_tx_inputs: ForkedParam<usize>,
        max_tx_outputs: ForkedParam<usize>,
        max_signature_script_len: ForkedParam<usize>,
        max_script_public_key_len: ForkedParam<usize>,
        coinbase_payload_script_public_key_max_len: u8,
        coinbase_maturity: ForkedParam<u64>,
        ghostdag_k: KType,
        counters: Arc<TxScriptCacheCounters>,
        mass_calculator: MassCalculator,
        crescendo_activation: ForkActivation,
    ) -> Self {
        Self {
            max_tx_inputs,
            max_tx_outputs,
            max_signature_script_len,
            max_script_public_key_len,
            coinbase_payload_script_public_key_max_len,
            coinbase_maturity,
            ghostdag_k,
            sig_cache: Cache::with_counters(10_000, counters),
            mass_calculator,
            crescendo_activation,
        }
    }

    pub fn new_for_tests(
        max_tx_inputs: usize,
        max_tx_outputs: usize,
        max_signature_script_len: usize,
        max_script_public_key_len: usize,
        coinbase_payload_script_public_key_max_len: u8,
        coinbase_maturity: u64,
        ghostdag_k: KType,
        counters: Arc<TxScriptCacheCounters>,
    ) -> Self {
        Self {
            max_tx_inputs: ForkedParam::new_const(max_tx_inputs),
            max_tx_outputs: ForkedParam::new_const(max_tx_outputs),
            max_signature_script_len: ForkedParam::new_const(max_signature_script_len),
            max_script_public_key_len: ForkedParam::new_const(max_script_public_key_len),
            coinbase_payload_script_public_key_max_len,
            coinbase_maturity: ForkedParam::new_const(coinbase_maturity),
            ghostdag_k,
            sig_cache: Cache::with_counters(10_000, counters),
            mass_calculator: MassCalculator::new(0, 0, 0, 0),
            crescendo_activation: ForkActivation::never(),
        }
    }
}
