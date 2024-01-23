//!
//! [`Generator`] async [`Stream`] implementation that produces pending transactions.
//!

use std::pin::Pin;
use std::task::{Context, Poll};

use crate::result::Result;
use crate::tx::{Generator, PendingTransaction};
use futures::Stream;

pub struct PendingTransactionStream {
    generator: Generator,
}

impl PendingTransactionStream {
    pub fn new(generator: &Generator) -> Self {
        Self { generator: generator.clone() }
    }
}

impl Stream for PendingTransactionStream {
    type Item = Result<PendingTransaction>;
    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.generator.generate_transaction().transpose())
    }
}
