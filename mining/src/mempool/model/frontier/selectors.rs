use crate::Policy;
use kaspa_consensus_core::{
    block::TemplateTransactionSelector,
    tx::{Transaction, TransactionId},
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

pub struct SequenceSelectorTransaction {
    pub tx: Arc<Transaction>,
    pub mass: u64,
}

impl SequenceSelectorTransaction {
    pub fn new(tx: Arc<Transaction>, mass: u64) -> Self {
        Self { tx, mass }
    }
}

type SequencePriorityIndex = u32;

/// The input sequence for the [`SequenceSelector`] transaction selector
#[derive(Default)]
pub struct SequenceSelectorInput {
    /// We use the btree map ordered by insertion order in order to follow
    /// the initial sequence order while allowing for efficient removal of previous selections
    inner: BTreeMap<SequencePriorityIndex, SequenceSelectorTransaction>,
}

impl FromIterator<SequenceSelectorTransaction> for SequenceSelectorInput {
    fn from_iter<T: IntoIterator<Item = SequenceSelectorTransaction>>(iter: T) -> Self {
        Self { inner: BTreeMap::from_iter(iter.into_iter().enumerate().map(|(i, v)| (i as SequencePriorityIndex, v))) }
    }
}

impl SequenceSelectorInput {
    pub fn push(&mut self, tx: Arc<Transaction>, mass: u64) {
        let idx = self.inner.len() as SequencePriorityIndex;
        self.inner.insert(idx, SequenceSelectorTransaction::new(tx, mass));
    }

    pub fn iter(&self) -> impl Iterator<Item = &SequenceSelectorTransaction> {
        self.inner.values()
    }
}

/// Helper struct for storing data related to previous selections
struct SequenceSelectorSelection {
    tx_id: TransactionId,
    mass: u64,
    priority_index: SequencePriorityIndex,
}

/// A selector which selects transactions in the order they are provided. The selector assumes
/// that the transactions were already selected via weighted sampling and simply tries them one
/// after the other until the block mass limit is reached.  
pub struct SequenceSelector {
    input_sequence: SequenceSelectorInput,
    selected_vec: Vec<SequenceSelectorSelection>,
    /// Maps from selected tx ids to tx mass so that the total used mass can be subtracted on tx reject
    selected_map: Option<HashMap<TransactionId, u64>>,
    total_selected_mass: u64,
    overall_candidates: usize,
    overall_rejections: usize,
    policy: Policy,
}

impl SequenceSelector {
    pub fn new(input_sequence: SequenceSelectorInput, policy: Policy) -> Self {
        Self {
            overall_candidates: input_sequence.inner.len(),
            selected_vec: Vec::with_capacity(input_sequence.inner.len()),
            input_sequence,
            selected_map: Default::default(),
            total_selected_mass: Default::default(),
            overall_rejections: Default::default(),
            policy,
        }
    }

    #[inline]
    fn reset_selection(&mut self) {
        self.selected_vec.clear();
        self.selected_map = None;
    }
}

impl TemplateTransactionSelector for SequenceSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        // Remove selections from the previous round if any
        for selection in self.selected_vec.drain(..) {
            self.input_sequence.inner.remove(&selection.priority_index);
        }
        // Reset selection data structures
        self.reset_selection();
        let mut transactions = Vec::with_capacity(self.input_sequence.inner.len());

        // Iterate the input sequence in order
        for (&priority_index, tx) in self.input_sequence.inner.iter() {
            if self.total_selected_mass.saturating_add(tx.mass) > self.policy.max_block_mass {
                // We assume the sequence is relatively small, hence we keep on searching
                // for transactions with lower mass which might fit into the remaining gap
                continue;
            }
            self.total_selected_mass += tx.mass;
            self.selected_vec.push(SequenceSelectorSelection { tx_id: tx.tx.id(), mass: tx.mass, priority_index });
            transactions.push(tx.tx.as_ref().clone())
        }
        transactions
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        // Lazy-create the map only when there are actual rejections
        let selected_map = self.selected_map.get_or_insert_with(|| self.selected_vec.iter().map(|tx| (tx.tx_id, tx.mass)).collect());
        let mass = selected_map.remove(&tx_id).expect("only previously selected txs can be rejected (and only once)");
        // Selections must be counted in total selected mass, so this subtraction cannot underflow
        self.total_selected_mass -= mass;
        self.overall_rejections += 1;
    }

    fn is_successful(&self) -> bool {
        const SUFFICIENT_MASS_THRESHOLD: f64 = 0.8;
        const LOW_REJECTION_FRACTION: f64 = 0.2;

        // We consider the operation successful if either mass occupation is above 80% or rejection rate is below 20%
        self.overall_rejections == 0
            || (self.total_selected_mass as f64) > self.policy.max_block_mass as f64 * SUFFICIENT_MASS_THRESHOLD
            || (self.overall_rejections as f64) < self.overall_candidates as f64 * LOW_REJECTION_FRACTION
    }
}

/// A selector that selects all the transactions it holds and is always considered successful.
/// If all mempool transactions have combined mass which is <= block mass limit, this selector
/// should be called and provided with all the transactions.
pub struct TakeAllSelector {
    txs: Vec<Arc<Transaction>>,
}

impl TakeAllSelector {
    pub fn new(txs: Vec<Arc<Transaction>>) -> Self {
        Self { txs }
    }
}

impl TemplateTransactionSelector for TakeAllSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        // Drain on the first call so that subsequent calls return nothing
        self.txs.drain(..).map(|tx| tx.as_ref().clone()).collect()
    }

    fn reject_selection(&mut self, _tx_id: TransactionId) {
        // No need to track rejections (for reduced mass), since there's nothing else to select
    }

    fn is_successful(&self) -> bool {
        // Considered successful because we provided all mempool transactions to this
        // selector, so there's no point in retries
        true
    }
}
