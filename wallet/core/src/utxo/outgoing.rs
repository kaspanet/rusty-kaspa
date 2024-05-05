//!
//! Implements the [`OutgoingTransaction`] type,
//! which is a wrapper around [`PendingTransaction`]
//! that adds additional transaction context information.
//!

use crate::imports::*;
use crate::tx::PendingTransaction;
use crate::utxo::{UtxoContext, UtxoEntryId, UtxoEntryReference};

struct Inner {
    pub id: TransactionId,
    pub pending_transaction: PendingTransaction,
    pub originating_context: UtxoContext,
    pub destination_context: Option<UtxoContext>,
    #[allow(dead_code)]
    pub creation_daa_score: u64,
    pub acceptance_daa_score: AtomicU64,
}

/// A wrapper around [`PendingTransaction`] that adds additional context and
/// convenience methods for handling within [`UtxoContext`].
#[derive(Clone)]
pub struct OutgoingTransaction {
    inner: Arc<Inner>,
}

impl OutgoingTransaction {
    pub fn new(current_daa_score: u64, originating_context: UtxoContext, pending_transaction: PendingTransaction) -> Self {
        let destination_context = pending_transaction.generator().destination_utxo_context().clone();

        let inner = Inner {
            id: pending_transaction.id(),
            pending_transaction,
            originating_context,
            destination_context,
            creation_daa_score: current_daa_score,
            acceptance_daa_score: AtomicU64::new(0),
        };

        Self { inner: Arc::new(inner) }
    }

    pub fn id(&self) -> TransactionId {
        self.inner.id
    }

    pub fn payment_value(&self) -> Option<u64> {
        self.inner.pending_transaction.payment_value()
    }

    pub fn fees(&self) -> u64 {
        self.inner.pending_transaction.fees()
    }

    pub fn aggregate_input_value(&self) -> u64 {
        self.inner.pending_transaction.aggregate_input_value()
    }

    pub fn aggregate_output_value(&self) -> u64 {
        self.inner.pending_transaction.aggregate_output_value()
    }

    pub fn pending_transaction(&self) -> &PendingTransaction {
        &self.inner.pending_transaction
    }

    pub fn tag_as_accepted_at_daa_score(&self, accepted_daa_score: u64) {
        self.inner.acceptance_daa_score.store(accepted_daa_score, Ordering::Relaxed);
    }

    pub fn acceptance_daa_score(&self) -> u64 {
        self.inner.acceptance_daa_score.load(Ordering::Relaxed)
    }

    pub fn is_accepted(&self) -> bool {
        self.inner.acceptance_daa_score.load(Ordering::Relaxed) != 0
    }

    pub fn is_batch(&self) -> bool {
        self.inner.pending_transaction.is_batch()
    }

    pub fn utxo_entries(&self) -> &AHashMap<UtxoEntryId, UtxoEntryReference> {
        self.inner.pending_transaction.utxo_entries()
    }

    pub fn originating_context(&self) -> &UtxoContext {
        &self.inner.originating_context
    }

    pub fn destination_context(&self) -> &Option<UtxoContext> {
        &self.inner.destination_context
    }
}

impl Eq for OutgoingTransaction {}

impl PartialEq for OutgoingTransaction {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl std::hash::Hash for OutgoingTransaction {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}
