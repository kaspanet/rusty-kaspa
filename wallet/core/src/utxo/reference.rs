use crate::imports::*;
use crate::utxo::{
    UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA, UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA,
    UTXO_STASIS_PERIOD_COINBASE_TRANSACTION_DAA,
};
pub use kaspa_consensus_wasm::{TryIntoUtxoEntryReferences, UtxoEntryReference};

pub enum Maturity {
    /// Coinbase UTXO that has not reached [`UTXO_STASIS_PERIOD_COINBASE_TRANSACTION_DAA`]
    Stasis,
    /// Coinbase UTXO that has reached [`UTXO_STASIS_PERIOD_COINBASE_TRANSACTION_DAA`]
    /// but has not reached [`UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA`] or
    /// user UTXO that has not reached [`UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA`]
    Pending,
    /// UTXO that has reached [`UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA`] or
    /// [`UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA`] respectively.
    Confirmed,
}

impl std::fmt::Display for Maturity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Maturity::Stasis => write!(f, "stasis"),
            Maturity::Pending => write!(f, "pending"),
            Maturity::Confirmed => write!(f, "confirmed"),
        }
    }
}

pub trait UtxoEntryReferenceExtension {
    fn maturity(&self, current_daa_score: u64) -> Maturity;
    fn balance(&self, current_daa_score: u64) -> Balance;
}

impl UtxoEntryReferenceExtension for UtxoEntryReference {
    fn maturity(&self, current_daa_score: u64) -> Maturity {
        if self.is_coinbase() {
            if self.block_daa_score() + UTXO_STASIS_PERIOD_COINBASE_TRANSACTION_DAA.load(Ordering::SeqCst) > current_daa_score {
                Maturity::Stasis
            } else if self.block_daa_score() + UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA.load(Ordering::SeqCst) > current_daa_score
            {
                Maturity::Pending
            } else {
                Maturity::Confirmed
            }
        } else if self.block_daa_score() + UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA.load(Ordering::SeqCst) > current_daa_score {
            Maturity::Pending
        } else {
            Maturity::Confirmed
        }
    }

    fn balance(&self, current_daa_score: u64) -> Balance {
        match self.maturity(current_daa_score) {
            Maturity::Pending => Balance::new(0, self.amount(), self.amount(), 0, 1, 0),
            Maturity::Stasis => Balance::new(0, 0, 0, 0, 0, 1),
            Maturity::Confirmed => Balance::new(self.amount(), 0, 0, 1, 0, 0),
        }
    }
}
