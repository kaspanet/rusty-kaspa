use crate::mempool::tx::{Priority, RbfPolicy};
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint};
use kaspa_mining_errors::mempool::RuleError;
use std::{
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
}

impl RbfPolicy {
    #[cfg(test)]
    /// Returns an alternate policy accepting a transaction insertion in case the policy requires a replacement
    pub(crate) fn for_insert(&self) -> RbfPolicy {
        match self {
            RbfPolicy::Forbidden | RbfPolicy::Allowed => *self,
            RbfPolicy::Mandatory => RbfPolicy::Allowed,
        }
    }
}

pub(crate) struct DoubleSpend {
    pub outpoint: TransactionOutpoint,
    pub owner_id: TransactionId,
}

impl DoubleSpend {
    pub fn new(outpoint: TransactionOutpoint, owner_id: TransactionId) -> Self {
        Self { outpoint, owner_id }
    }
}

impl From<DoubleSpend> for RuleError {
    fn from(value: DoubleSpend) -> Self {
        RuleError::RejectDoubleSpendInMempool(value.outpoint, value.owner_id)
    }
}

impl From<&DoubleSpend> for RuleError {
    fn from(value: &DoubleSpend) -> Self {
        RuleError::RejectDoubleSpendInMempool(value.outpoint, value.owner_id)
    }
}

pub(crate) struct TransactionPreValidation {
    pub transaction: MutableTransaction,
    pub feerate_threshold: Option<f64>,
}

#[derive(Default)]
pub(crate) struct TransactionPostValidation {
    pub removed: Option<Arc<Transaction>>,
    pub accepted: Option<Arc<Transaction>>,
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
