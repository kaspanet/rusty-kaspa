use crate::mempool::tx::Priority;
use kaspa_consensus_core::{tx::MutableTransaction, tx::TransactionId};
use std::{
    cmp::Ordering,
    fmt::{Display, Formatter},
};

pub(crate) struct MempoolTransaction {
    pub(crate) mtx: MutableTransaction,
    pub(crate) priority: Priority,
    pub(crate) added_at_daa_score: u64,
}

impl MempoolTransaction {
    pub(crate) fn new(mtx: MutableTransaction, priority: Priority, added_at_daa_score: u64) -> Self {
        assert_eq!(mtx.tx.inputs.len(), mtx.entries.len());
        Self { mtx, priority, added_at_daa_score }
    }

    pub(crate) fn id(&self) -> TransactionId {
        self.mtx.tx.id()
    }

    pub(crate) fn fee_rate(&self) -> f64 {
        self.mtx.calculated_fee.unwrap() as f64 / self.mtx.calculated_mass.unwrap() as f64
    }

    pub(crate) fn is_parent_of(&self, transaction: &MutableTransaction) -> bool {
        let parent_id = self.id();
        transaction.tx.inputs.iter().any(|x| x.previous_outpoint.transaction_id == parent_id)
    }
}

impl Ord for MempoolTransaction {
    fn cmp(&self, other: &Self) -> Ordering {
        self.fee_rate().total_cmp(&other.fee_rate()).then(self.id().cmp(&other.id()))
    }
}

impl Eq for MempoolTransaction {}

impl PartialOrd for MempoolTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for MempoolTransaction {
    fn eq(&self, other: &Self) -> bool {
        self.fee_rate() == other.fee_rate()
    }
}

#[derive(PartialEq, Eq)]
pub(crate) enum TxRemovalReason {
    Muted,
    Accepted,
    MakingRoom,
    Unorphaned,
    Expired,
    DoubleSpend,
    InvalidInBlockTemplate,
    RevalidationWithMissingOutpoints,
}

impl TxRemovalReason {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            TxRemovalReason::Muted => "",
            TxRemovalReason::Accepted => "accepted",
            TxRemovalReason::MakingRoom => "making room",
            TxRemovalReason::Unorphaned => "unorphaned",
            TxRemovalReason::Expired => "expired",
            TxRemovalReason::DoubleSpend => "double spend",
            TxRemovalReason::InvalidInBlockTemplate => "invalid in block template",
            TxRemovalReason::RevalidationWithMissingOutpoints => "revalidation with missing outpoints",
        }
    }

    pub(crate) fn verbose(&self) -> bool {
        !matches!(self, TxRemovalReason::Muted)
    }
}

impl Display for TxRemovalReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
