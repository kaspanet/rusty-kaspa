use crate::stores::RefType;
use kaspa_consensus_core::{
    acceptance_data::MergesetIndexType,
    tx::{TransactionId, TransactionIndexType},
    Hash,
};

pub type TxAcceptedTuple = (TransactionId, u64, Hash, MergesetIndexType);

pub struct TxAcceptedIter<I>(I);

impl<I> Iterator for TxAcceptedIter<I>
where
    I: Iterator<Item = TxAcceptedTuple>,
{
    type Item = TxAcceptedTuple;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}
