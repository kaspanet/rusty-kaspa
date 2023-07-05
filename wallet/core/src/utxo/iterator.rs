use super::{UtxoContext, UtxoEntryReference};
use crate::imports::*;

pub struct UtxoSetIterator {
    utxo_context: UtxoContext,
    cursor: usize,
}

impl UtxoSetIterator {
    pub fn new(utxo_context: &UtxoContext) -> Self {
        Self { utxo_context: utxo_context.clone(), cursor: 0 }
    }
}

impl Stream for UtxoSetIterator {
    type Item = UtxoEntryReference;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let entry = self.utxo_context.inner.lock().unwrap().mature.get(self.cursor).cloned();
        self.cursor += 1;
        Poll::Ready(entry)
    }
}
