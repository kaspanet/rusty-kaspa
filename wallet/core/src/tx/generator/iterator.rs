//!
//! [`Generator`] Iterator implementation that produces pending transactions.
//!

use crate::result::Result;
use crate::tx::{Generator, PendingTransaction};

pub struct PendingTransactionIterator {
    generator: Generator,
}

impl PendingTransactionIterator {
    pub fn new(generator: &Generator) -> Self {
        Self { generator: generator.clone() }
    }
}

impl Iterator for PendingTransactionIterator {
    type Item = Result<PendingTransaction>;
    fn next(&mut self) -> Option<Self::Item> {
        self.generator.generate_transaction().transpose()
    }
}
