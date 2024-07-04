use crate::mempool::tx::Priority;
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, TransactionId};
use std::{
    cmp::Ordering,
    fmt::{Display, Formatter},
    sync::Arc,
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
        let contextual_mass = self.mtx.tx.mass();
        assert!(contextual_mass > 0, "expected to be called for validated txs only");
        self.mtx.calculated_fee.unwrap() as f64 / contextual_mass as f64
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

/// Replace by Fee policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RbfPolicy {
    /// RBF is forbidden, fails on double spend
    Forbidden,
    /// RBF may occur, fails on double spend when not all RBF conditions are met
    Allowed,
    /// RBF must occur, fails if no transaction can be replaced
    Mandatory,
}

#[derive(Default)]
pub(crate) struct TransactionInsertOutcome {
    pub removed: Option<Arc<Transaction>>,
    pub accepted: Option<Arc<Transaction>>,
}

impl TransactionInsertOutcome {
    pub(crate) fn new(removed: Option<Arc<Transaction>>, accepted: Option<Arc<Transaction>>) -> Self {
        Self { removed, accepted }
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
    ReplacedByFee,
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
            TxRemovalReason::ReplacedByFee => "replaced by fee",
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
