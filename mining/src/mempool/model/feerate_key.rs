use kaspa_consensus_core::tx::TransactionId;

use super::tx::MempoolTransaction;

#[derive(Clone, Debug)]
pub struct FeerateTransactionKey {
    pub fee: u64,
    pub mass: u64,
    pub id: TransactionId,
}

impl Eq for FeerateTransactionKey {}

impl PartialEq for FeerateTransactionKey {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl FeerateTransactionKey {
    pub fn feerate(&self) -> f64 {
        self.fee as f64 / self.mass as f64
    }
}

impl std::hash::Hash for FeerateTransactionKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Transaction id is a sufficient identifier for this key
        self.id.hash(state);
    }
}

impl PartialOrd for FeerateTransactionKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FeerateTransactionKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Our first priority is the feerate
        match self.feerate().total_cmp(&other.feerate()) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }

        // If feerates are equal, prefer the higher fee in absolute value
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
        self.id.cmp(&other.id)
    }
}

impl From<&MempoolTransaction> for FeerateTransactionKey {
    fn from(tx: &MempoolTransaction) -> Self {
        Self { fee: tx.mtx.calculated_fee.unwrap(), mass: tx.mtx.tx.mass(), id: tx.id() }
    }
}
