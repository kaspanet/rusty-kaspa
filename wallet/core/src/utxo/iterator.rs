use super::{UtxoEntryReference, UtxoProcessor};
use crate::imports::*;

pub struct UtxoSetIterator {
    utxo_processor: UtxoProcessor,
    cursor: usize,
}

impl UtxoSetIterator {
    pub fn new(utxo_processor: &UtxoProcessor) -> Self {
        Self { utxo_processor: utxo_processor.clone(), cursor: 0 }
    }
}

impl Stream for UtxoSetIterator {
    type Item = UtxoEntryReference;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let entry = self.utxo_processor.inner.lock().unwrap().mature.get(self.cursor).cloned();
        self.cursor += 1;
        Poll::Ready(entry)
    }
}
