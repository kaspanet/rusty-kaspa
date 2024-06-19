//!
//! Implements the [`PendingUtxoEntryReference`] type used
//! by the [`UtxoProcessor`] to monitor UTXO maturity progress.
//!

use crate::imports::*;
use crate::utxo::{Maturity, UtxoContext, UtxoEntryId, UtxoEntryReference, UtxoEntryReferenceExtension};

pub struct PendingUtxoEntryReferenceInner<RpcImpl> {
    pub entry: UtxoEntryReference,
    pub utxo_context: UtxoContext<RpcImpl>,
}

#[derive(Clone)]
pub struct PendingUtxoEntryReference<RpcImpl> {
    pub inner: Arc<PendingUtxoEntryReferenceInner<RpcImpl>>,
}

impl<RpcImpl> PendingUtxoEntryReference<RpcImpl> {
    pub fn new(entry: UtxoEntryReference, utxo_context: UtxoContext<RpcImpl>) -> Self {
        Self { inner: Arc::new(PendingUtxoEntryReferenceInner { entry, utxo_context }) }
    }

    #[inline(always)]
    pub fn inner(&self) -> &PendingUtxoEntryReferenceInner<RpcImpl> {
        &self.inner
    }

    #[inline(always)]
    pub fn entry(&self) -> &UtxoEntryReference {
        &self.inner().entry
    }

    #[inline(always)]
    pub fn utxo_context(&self) -> &UtxoContext<RpcImpl> {
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

impl<RpcImpl> From<(&Arc<dyn Account>, UtxoEntryReference)> for PendingUtxoEntryReference<RpcImpl> {
    fn from((account, entry): (&Arc<dyn Account>, UtxoEntryReference)) -> Self {
        Self::new(entry, (*account.utxo_context()).clone())
    }
}

impl<RpcImpl> From<PendingUtxoEntryReference<RpcImpl>> for UtxoEntryReference {
    fn from(pending: PendingUtxoEntryReference<RpcImpl>) -> Self {
        pending.inner().entry.clone()
    }
}
