//!
//! Implements the [`PendingUtxoEntryReference`] type used
//! by the [`UtxoProcessor`] to monitor UTXO maturity progress.
//!

use crate::imports::*;
use crate::utxo::{Maturity, UtxoContext, UtxoEntryId, UtxoEntryReference, UtxoEntryReferenceExtension};

pub struct PendingUtxoEntryReferenceInner {
    pub entry: UtxoEntryReference,
    pub utxo_context: UtxoContext,
}

#[derive(Clone)]
pub struct PendingUtxoEntryReference {
    pub inner: Arc<PendingUtxoEntryReferenceInner>,
}

impl PendingUtxoEntryReference {
    pub fn new(entry: UtxoEntryReference, utxo_context: UtxoContext) -> Self {
        Self { inner: Arc::new(PendingUtxoEntryReferenceInner { entry, utxo_context }) }
    }

    #[inline(always)]
    pub fn inner(&self) -> &PendingUtxoEntryReferenceInner {
        &self.inner
    }

    #[inline(always)]
    pub fn entry(&self) -> &UtxoEntryReference {
        &self.inner().entry
    }

    #[inline(always)]
    pub fn utxo_context(&self) -> &UtxoContext {
        &self.inner().utxo_context
    }

    #[inline(always)]
    pub fn id(&self) -> UtxoEntryId {
        self.inner().entry.id()
    }

    #[inline(always)]
    pub fn transaction_id(&self) -> TransactionId {
        self.inner().entry.transaction_id()
    }

    #[inline(always)]
    pub fn maturity(&self, params: &NetworkParams, current_daa_score: u64) -> Maturity {
        self.inner().entry.maturity(params, current_daa_score)
    }
}

impl From<(&Arc<dyn Account>, UtxoEntryReference)> for PendingUtxoEntryReference {
    fn from((account, entry): (&Arc<dyn Account>, UtxoEntryReference)) -> Self {
        Self::new(entry, (*account.utxo_context()).clone())
    }
}

impl From<PendingUtxoEntryReference> for UtxoEntryReference {
    fn from(pending: PendingUtxoEntryReference) -> Self {
        pending.inner().entry.clone()
    }
}
