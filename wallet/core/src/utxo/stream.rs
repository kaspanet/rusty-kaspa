//!
//! Implements an async stream of UTXOs.
//!

use super::{UtxoContext, UtxoEntryReference};
use crate::imports::*;

pub struct UtxoStream {
    utxo_context: UtxoContext,
    cursor: usize,
}

impl UtxoStream {
    pub fn new(utxo_context: &UtxoContext) -> Self {
        Self { utxo_context: utxo_context.clone(), cursor: 0 }
    }
}

impl Stream for UtxoStream {
    type Item = UtxoEntryReference;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let entry = self.utxo_context.context().mature.get(self.cursor).cloned();
        self.cursor += 1;
        Poll::Ready(entry)
    }
}
