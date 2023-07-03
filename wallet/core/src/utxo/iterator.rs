use super::{UtxoDb, UtxoEntryReference};
use crate::imports::*;

pub struct UtxoSetIterator {
    utxos: UtxoDb,
    cursor: usize,
}

impl UtxoSetIterator {
    pub fn new(utxos: UtxoDb) -> Self {
        Self { utxos, cursor: 0 }
    }
}

impl Stream for UtxoSetIterator {
    type Item = UtxoEntryReference;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let entry = self.utxos.inner.lock().unwrap().mature.get(self.cursor).cloned();
        self.cursor += 1;
        Poll::Ready(entry)
    }
}
