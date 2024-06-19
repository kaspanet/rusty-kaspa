//!
//! [`Generator`] Iterator implementation that produces pending transactions.
//!

use crate::result::Result;
use crate::tx::{Generator, PendingTransaction};

pub struct PendingTransactionIterator<RpcImpl> {
    generator: Generator<RpcImpl>,
}

impl<RpcImpl> PendingTransactionIterator<RpcImpl> {
    pub fn new(generator: &Generator<RpcImpl>) -> Self {
        Self { generator: generator.clone() }
    }
}

impl<RpcImpl> Iterator for PendingTransactionIterator<RpcImpl> {
    type Item = Result<PendingTransaction>;
    fn next(&mut self) -> Option<Self::Item> {
        self.generator.generate_transaction().transpose()
    }
}
