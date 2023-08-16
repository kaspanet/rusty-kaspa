use crate::imports::*;
use crate::runtime::Account;
use crate::utxo::{UtxoContext, UtxoEntryId, UtxoEntryReference, UtxoEntryReferenceExtension};

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
    pub fn is_mature(&self, current_daa_score: u64) -> bool {
        self.inner().entry.is_mature(current_daa_score)
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

// ---

/// A simple collection of UTXO entries. This struct is used to
/// retain a set of UTXO entries in the WASM memory for faster
/// processing. This struct keeps a list of entries represented
/// by `UtxoEntryReference` struct. This data structure is used
/// internally by the framework, but is exposed for convenience.
/// Please consider using `UtxoContect` instead.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen(inspectable)]
pub struct UtxoEntries(Arc<Vec<UtxoEntryReference>>);

impl UtxoEntries {
    pub fn contains(&self, entry: &UtxoEntryReference) -> bool {
        self.0.contains(entry)
    }

    pub fn iter(&self) -> impl Iterator<Item = &UtxoEntryReference> {
        self.0.iter()
    }
}
