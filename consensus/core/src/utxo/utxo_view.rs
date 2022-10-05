use super::utxo_diff::*;
use crate::tx::*;

/// An abstraction for read-only queries over a UTXO collection
pub trait UtxoView {
    fn get(&self, outpoint: &TransactionOutpoint) -> Option<&UtxoEntry>;
}

/// Composes a UTXO view from a base UTXO view and a UTXO diff
/// Note: can be used to compose any number of diff layers by nesting instances
pub struct ComposedUtxoView<V: UtxoView, D: ImmutableUtxoDiff> {
    base: V,
    diff: D,
}

impl<V: UtxoView, D: ImmutableUtxoDiff> ComposedUtxoView<V, D> {
    pub fn new(base: V, diff: D) -> Self {
        Self { base, diff }
    }
}

impl<V: UtxoView, D: ImmutableUtxoDiff> UtxoView for ComposedUtxoView<V, D> {
    fn get(&self, outpoint: &TransactionOutpoint) -> Option<&UtxoEntry> {
        // First check diff added entries
        if let Some(entry) = self.diff.added().get(outpoint) {
            return Some(entry);
        }
        // If not in added, but in removed, then considered removed
        if self.diff.removed().contains_key(outpoint) {
            return None;
        }
        // Fallback to base view
        self.base.get(outpoint)
    }
}

impl<T: UtxoView> UtxoView for &T {
    fn get(&self, outpoint: &TransactionOutpoint) -> Option<&UtxoEntry> {
        (*self).get(outpoint)
    }
}

pub fn compose_one_diff_layer<B: UtxoView, D1: ImmutableUtxoDiff>(base: B, diff1: D1) -> ComposedUtxoView<B, D1> {
    ComposedUtxoView::new(base, diff1)
}

pub fn compose_two_diff_layers<B: UtxoView, D1: ImmutableUtxoDiff, D2: ImmutableUtxoDiff>(
    base: B,
    diff1: D1,
    diff2: D2,
) -> ComposedUtxoView<ComposedUtxoView<B, D1>, D2> {
    ComposedUtxoView::new(ComposedUtxoView::new(base, diff1), diff2)
}

pub fn compose_three_diff_layers<B: UtxoView, D1: ImmutableUtxoDiff, D2: ImmutableUtxoDiff, D3: ImmutableUtxoDiff>(
    base: B,
    diff1: D1,
    diff2: D2,
    diff3: D3,
) -> ComposedUtxoView<ComposedUtxoView<ComposedUtxoView<B, D1>, D2>, D3> {
    ComposedUtxoView::new(ComposedUtxoView::new(ComposedUtxoView::new(base, diff1), diff2), diff3)
}
