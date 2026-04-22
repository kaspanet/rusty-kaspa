use super::search_tree::SearchTree;
use crate::Policy;
use kaspa_consensus_core::{
    block::TemplateTransactionSelector,
    subnets::SubnetworkId,
    tx::{Transaction, TransactionId},
};
use rand::Rng;
use std::{
    collections::{BTreeMap, HashMap, hash_map::Entry},
    sync::Arc,
};

#[derive(Default)]
struct LaneUsage {
    tx_count: usize,
    gas: u64,
}

#[derive(Default)]
struct LaneSelectionState {
    occupied: HashMap<SubnetworkId, LaneUsage>,
}

impl LaneSelectionState {
    // LPB and gas are enforced during selection, but gas is intentionally not part of the
    // global feerate weight since gas capacity is lane-local.
    fn try_select(&mut self, policy: &Policy, lane: SubnetworkId, gas: u64) -> bool {
        let occupied_len = self.occupied.len();
        match self.occupied.entry(lane) {
            Entry::Occupied(mut entry) => {
                let usage = entry.get_mut();
                if usage.gas.saturating_add(gas) > policy.gas_per_lane_limit {
                    return false;
                }
                usage.tx_count += 1;
                usage.gas += gas;
                true
            }
            Entry::Vacant(entry) => {
                if occupied_len >= policy.lanes_per_block_limit || gas > policy.gas_per_lane_limit {
                    return false;
                }
                entry.insert(LaneUsage { tx_count: 1, gas });
                true
            }
        }
    }

    fn reject(&mut self, lane: SubnetworkId, gas: u64) {
        let usage = self.occupied.get_mut(&lane).expect("previously selected txs occupy a lane");
        usage.tx_count -= 1;
        usage.gas -= gas;
        if usage.tx_count == 0 {
            self.occupied.remove(&lane);
        }
    }
}

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
    lane: SubnetworkId,
    gas: u64,
    priority_index: SequencePriorityIndex,
}

/// A selector which selects transactions in the order they are provided. The selector assumes
/// that the transactions were already selected via weighted sampling and simply tries them one
/// after the other until the block mass limit is reached.
///
/// The input sequence is expected to already be ordered by the chosen sampling strategy.
/// `SequenceSelector` then enforces block mass, LPB, gas, and rejection/retry behavior.
pub struct SequenceSelector {
    input_sequence: SequenceSelectorInput,
    selected_vec: Vec<SequenceSelectorSelection>,
    /// Maps from selected tx ids to resource usage so it can be subtracted on tx reject
    selected_map: Option<HashMap<TransactionId, (u64, SubnetworkId, u64)>>,
    total_selected_mass: u64,
    lanes: LaneSelectionState,
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
            lanes: Default::default(),
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
            if !self.lanes.try_select(&self.policy, tx.tx.subnetwork_id, tx.tx.gas) {
                continue;
            }
            self.total_selected_mass += tx.mass;
            self.selected_vec.push(SequenceSelectorSelection {
                tx_id: tx.tx.id(),
                mass: tx.mass,
                lane: tx.tx.subnetwork_id,
                gas: tx.tx.gas,
                priority_index,
            });
            transactions.push(tx.tx.as_ref().clone())
        }
        transactions
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        // Lazy-create the map only when there are actual rejections
        let selected_map = self
            .selected_map
            .get_or_insert_with(|| self.selected_vec.iter().map(|tx| (tx.tx_id, (tx.mass, tx.lane, tx.gas))).collect());
        let (mass, lane, gas) = selected_map.remove(&tx_id).expect("only previously selected txs can be rejected (and only once)");
        // Selections must be counted in total selected mass, so this subtraction cannot underflow
        self.total_selected_mass -= mass;
        self.lanes.reject(lane, gas);
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
    policy: Policy,
}

impl TakeAllSelector {
    pub fn new(txs: Vec<Arc<Transaction>>, policy: Policy) -> Self {
        Self { txs, policy }
    }
}

impl TemplateTransactionSelector for TakeAllSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        // Drain on the first call so that subsequent calls return nothing.
        // This simple path currently compromises on retry optimality: LPB/gas-skipped
        // txs are not reconsidered after later rejections (the mempool is less congested
        // here and tx rejections are expected to be rare).
        let mut lanes = LaneSelectionState::default();
        self.txs
            .drain(..)
            .filter_map(|tx| if lanes.try_select(&self.policy, tx.subnetwork_id, tx.gas) { Some(tx.as_ref().clone()) } else { None })
            .collect()
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

struct TreeSelectorSelection {
    tx_id: TransactionId,
    mass: u64,
    lane: SubnetworkId,
    gas: u64,
}

/// A weighted selector over a local mutable search tree.
///
/// This is intended as a simpler replacement candidate for the rebalancing selector: instead of
/// lazily marking sampled candidates and periodically rebuilding the candidate list, it removes
/// candidates from a local tree as they are selected or skipped by LPB/gas limits.
pub struct MutatingTreeSelector {
    tree: SearchTree,
    selected_vec: Vec<TreeSelectorSelection>,
    selected_map: Option<HashMap<TransactionId, TreeSelectorSelection>>,
    total_selected_mass: u64,
    lanes: LaneSelectionState,
    overall_candidates: usize,
    overall_rejections: usize,
    policy: Policy,
}

impl MutatingTreeSelector {
    pub fn new(policy: Policy, tree: SearchTree) -> Self {
        let overall_candidates = tree.len();
        Self {
            tree,
            selected_vec: Vec::with_capacity(overall_candidates),
            selected_map: None,
            total_selected_mass: 0,
            lanes: Default::default(),
            overall_candidates,
            overall_rejections: 0,
            policy,
        }
    }

    fn reset_selection(&mut self) {
        self.selected_vec.clear();
        self.selected_map = None;
    }
}

impl TemplateTransactionSelector for MutatingTreeSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        self.reset_selection();
        let mut rng = rand::thread_rng();
        let mut transactions = Vec::new();

        while !self.tree.is_empty() {
            let query = rng.r#gen::<f64>() * self.tree.total_weight();
            let candidate = self.tree.search(query).clone();
            let tx = candidate.tx.as_ref();
            let lane = candidate.lane();

            let next_total_mass = self.total_selected_mass.saturating_add(candidate.mass);
            if next_total_mass > self.policy.max_block_mass {
                break;
            }

            if !self.lanes.try_select(&self.policy, lane, tx.gas) {
                // For now we compromise on retry optimality in this less-congested path:
                // LPB/gas-skipped candidates are not reconsidered after later rejections
                // (tx rejections are expected to be rare here).
                self.tree.remove(&candidate);
                continue;
            }

            self.tree.remove(&candidate);
            self.total_selected_mass = next_total_mass;
            self.selected_vec.push(TreeSelectorSelection { tx_id: tx.id(), mass: candidate.mass, lane, gas: tx.gas });
            transactions.push(tx.clone());
        }

        transactions
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        let selected_map = self
            .selected_map
            .get_or_insert_with(|| self.selected_vec.drain(..).map(|selection| (selection.tx_id, selection)).collect());
        let selection = selected_map.remove(&tx_id).expect("only previously selected txs can be rejected (and only once)");

        self.total_selected_mass -= selection.mass;
        self.lanes.reject(selection.lane, selection.gas);
        self.overall_rejections += 1;
    }

    fn is_successful(&self) -> bool {
        const SUFFICIENT_MASS_THRESHOLD: f64 = 0.8;
        const LOW_REJECTION_FRACTION: f64 = 0.2;

        self.overall_rejections == 0
            || (self.total_selected_mass as f64) > self.policy.max_block_mass as f64 * SUFFICIENT_MASS_THRESHOLD
            || (self.overall_rejections as f64) < self.overall_candidates as f64 * LOW_REJECTION_FRACTION
    }
}

#[cfg(test)]
mod tests {
    use super::super::feerate_key::FeerateTransactionKey;
    use super::*;
    use kaspa_consensus_core::tx::{TransactionInput, TransactionOutpoint};
    use kaspa_hashes::{HasherBase, TransactionID};

    fn lane(id: u8) -> SubnetworkId {
        SubnetworkId::from_namespace([id, 1, 0, 0])
    }

    fn tx(id: u64, lane: SubnetworkId, gas: u64) -> Arc<Transaction> {
        let mut hasher = TransactionID::new();
        let prev = hasher.update(id.to_le_bytes()).clone().finalize();
        let input = TransactionInput::new(TransactionOutpoint::new(prev, 0), vec![], 0, 0);
        Arc::new(Transaction::new(0, vec![input], vec![], 0, lane, gas, vec![]))
    }

    fn policy() -> Policy {
        let mut policy = Policy::new(100_000);
        policy.lanes_per_block_limit = 2;
        policy.gas_per_lane_limit = 10;
        policy
    }

    #[test]
    fn test_take_all_selector_respects_gas_limit() {
        let lane = lane(1);
        let txs = vec![tx(1, lane, 6), tx(2, lane, 6)];
        let mut selector = TakeAllSelector::new(txs, policy());
        let selected = selector.select_transactions();

        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn test_sequence_selector_respects_gas_limit_and_releases_on_reject() {
        let lane = lane(1);
        let input =
            vec![SequenceSelectorTransaction::new(tx(1, lane, 6), 1000), SequenceSelectorTransaction::new(tx(2, lane, 6), 1000)]
                .into_iter()
                .collect();
        let mut selector = SequenceSelector::new(input, policy());

        let selected = selector.select_transactions();
        assert_eq!(selected.len(), 1);

        selector.reject_selection(selected[0].id());
        let selected = selector.select_transactions();
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn test_mutating_tree_selector_continues_from_remaining_tree_after_reject() {
        let mut tree = SearchTree::new();
        tree.insert(FeerateTransactionKey::new(100, 1000, tx(1, lane(1), 0)));
        tree.insert(FeerateTransactionKey::new(100, 1000, tx(2, lane(1), 0)));
        tree.insert(FeerateTransactionKey::new(100, 1000, tx(3, lane(1), 0)));

        let mut policy = policy();
        policy.max_block_mass = 1000;
        let mut selector = MutatingTreeSelector::new(policy, tree);
        let selected = selector.select_transactions();
        assert_eq!(selected.len(), 1);
        let first_selected_id = selected[0].id();

        selector.reject_selection(first_selected_id);
        let selected = selector.select_transactions();
        assert_eq!(selected.len(), 1);
        assert_ne!(selected[0].id(), first_selected_id);
    }
}
