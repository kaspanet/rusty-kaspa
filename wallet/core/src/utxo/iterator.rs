//!
//! Associative iterator over the UTXO set.
//!

use crate::utxo::{UtxoContext, UtxoEntryReference};

#[derive(Debug)]
pub struct UtxoIterator {
    entries: Vec<UtxoEntryReference>,
    cursor: usize,
}

impl UtxoIterator {
    pub fn new(utxo_context: &UtxoContext) -> Self {
        Self { entries: utxo_context.context().mature.clone(), cursor: 0 }
    }
}

impl Iterator for UtxoIterator {
    type Item = UtxoEntryReference;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.entries.get(self.cursor).cloned();
        self.cursor += 1;
        entry
    }
}
