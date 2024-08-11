use crate::{block_template::selector::ALPHA, mempool::model::tx::MempoolTransaction};
use kaspa_consensus_core::tx::Transaction;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct FeerateTransactionKey {
    pub fee: u64,
    pub mass: u64,
    weight: f64,
    pub tx: Arc<Transaction>,
}

impl Eq for FeerateTransactionKey {}

impl PartialEq for FeerateTransactionKey {
    fn eq(&self, other: &Self) -> bool {
        self.tx.id() == other.tx.id()
    }
}

impl FeerateTransactionKey {
    pub fn new(fee: u64, mass: u64, tx: Arc<Transaction>) -> Self {
        // NOTE: any change to the way this weight is calculated (such as scaling by some factor)
        // requires a reversed update to total_weight in `Frontier::build_feerate_estimator`. This
        // is because the math methods in FeeEstimator assume this specific weight function.
        Self { fee, mass, weight: (fee as f64 / mass as f64).powi(ALPHA), tx }
    }

    pub fn feerate(&self) -> f64 {
        self.fee as f64 / self.mass as f64
    }

    pub fn weight(&self) -> f64 {
        self.weight
    }
}

impl std::hash::Hash for FeerateTransactionKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Transaction id is a sufficient identifier for this key
        self.tx.id().hash(state);
    }
}

impl PartialOrd for FeerateTransactionKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FeerateTransactionKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Our first priority is the feerate.
        // The weight function is monotonic in feerate so we prefer using it
        // since it is cached
        match self.weight().total_cmp(&other.weight()) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }

        // If feerates (and thus weights) are equal, prefer the higher fee in absolute value
        match self.fee.cmp(&other.fee) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }

        //
        // At this point we don't compare the mass fields since if both feerate
        // and fee are equal, mass must be equal as well
        //

        // Finally, we compare transaction ids in order to allow multiple transactions with
        // the same fee and mass to exist within the same sorted container
        self.tx.id().cmp(&other.tx.id())
    }
}

impl From<&MempoolTransaction> for FeerateTransactionKey {
    fn from(tx: &MempoolTransaction) -> Self {
        let mass = tx.mtx.tx.mass();
        let fee = tx.mtx.calculated_fee.expect("fee is expected to be populated");
        assert_ne!(mass, 0, "mass field is expected to be set when inserting to the mempool");
        Self::new(fee, mass, tx.mtx.tx.clone())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use kaspa_consensus_core::{
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{Transaction, TransactionInput, TransactionOutpoint},
    };
    use kaspa_hashes::{HasherBase, TransactionID};
    use std::sync::Arc;

    fn generate_unique_tx(i: u64) -> Arc<Transaction> {
        let mut hasher = TransactionID::new();
        let prev = hasher.update(i.to_le_bytes()).clone().finalize();
        let input = TransactionInput::new(TransactionOutpoint::new(prev, 0), vec![], 0, 0);
        Arc::new(Transaction::new(0, vec![input], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]))
    }

    /// Test helper for generating a feerate key with a unique tx (per u64 id)
    pub(crate) fn build_feerate_key(fee: u64, mass: u64, id: u64) -> FeerateTransactionKey {
        FeerateTransactionKey::new(fee, mass, generate_unique_tx(id))
    }
}
