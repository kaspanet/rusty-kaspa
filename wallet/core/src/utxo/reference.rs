use crate::imports::*;
use crate::runtime::Balance;
use crate::utxo::{UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA, UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA};
pub use kaspa_consensus_wasm::{TryIntoUtxoEntryReferences, UtxoEntryReference};

pub trait UtxoEntryReferenceExtension {
    fn is_mature(&self, current_daa_score: u64) -> bool;
    fn balance(&self, current_daa_score: u64) -> Balance;
}

impl UtxoEntryReferenceExtension for UtxoEntryReference {
    fn is_mature(&self, current_daa_score: u64) -> bool {
        if self.is_coinbase() {
            self.block_daa_score() + UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA.load(Ordering::SeqCst) < current_daa_score
        } else {
            self.block_daa_score() + UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA.load(Ordering::SeqCst) < current_daa_score
        }
    }

    fn balance(&self, current_daa_score: u64) -> Balance {
        if self.is_mature(current_daa_score) {
            Balance::new(self.amount(), 0, 0)
        } else {
            Balance::new(0, self.amount(), 0)
        }
    }
}
