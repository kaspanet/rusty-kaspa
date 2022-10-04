use super::utxo_diff::*;
use crate::tx::*;

pub trait UtxoView {
    fn get(&self, outpoint: &TransactionOutpoint) -> Option<&UtxoEntry>;
}

pub struct HierarchicUtxoView<'a, 'b, V: UtxoView, D: ImmutableUtxoDiff> {
    base: &'a V,
    diff: &'b D,
}

impl<'a, 'b, V: UtxoView, D: ImmutableUtxoDiff> HierarchicUtxoView<'a, 'b, V, D> {
    pub fn new(base: &'a V, diff: &'b D) -> Self {
        Self { base, diff }
    }
}

impl<'a, 'b, V: UtxoView, D: ImmutableUtxoDiff> UtxoView for HierarchicUtxoView<'a, 'b, V, D> {
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
