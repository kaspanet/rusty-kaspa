use crate::stores::{BlueScoreRefIter, BlueScoreRefTuple, RefType, TxInclusionIter, TxInclusionTuple};
use kaspa_consensus_core::tx::{TransactionId, TransactionIndexType};
use kaspa_consensus_core::Hash;
use kaspa_consensus_notify::notification::BlockAddedNotification;

pub struct ReindexedBlockAddedState<TxIter, BlueScoreIter>
where
    TxIter: Iterator<Item = TxInclusionTuple>,
    BlueScoreIter: Iterator<Item = BlueScoreRefTuple>,
{
    pub block_hash: Hash,
    pub direct_parents: Vec<Hash>,
    pub tx_iter: TxInclusionIter<TxIter>,
    pub blue_score_ref_iter: BlueScoreRefIter<BlueScoreIter>,
}

/// Returns the block hash and an iterator over (TransactionId, u64, Hash, TransactionIndexType) for the given BlockAddedNotification.
pub fn reindex_block_added_notification<'a>(
    block_added_notification: &'a BlockAddedNotification,
) -> ReindexedBlockAddedState<impl Iterator<Item = TxInclusionTuple> + 'a, impl Iterator<Item = BlueScoreRefTuple> + 'a> {
    let block_hash = block_added_notification.block.header.hash;
    let blue_score = block_added_notification.block.header.blue_score;
    let tx_iter = block_added_notification
        .block
        .transactions
        .iter()
        .enumerate()
        .map(move |(index_within_block, tx)| (tx.id(), blue_score, block_hash, index_within_block as TransactionIndexType));
    let blue_score_ref_iter = block_added_notification
        .block
        .transactions
        .iter()
        .map(move |tx| (block_added_notification.block.header.blue_score, RefType::Inclusion, tx.id()));
    ReindexedBlockAddedState {
        block_hash,
        direct_parents: block_added_notification.block.header.direct_parents().to_vec(),
        tx_iter: TxInclusionIter::new(tx_iter),
        blue_score_ref_iter: BlueScoreRefIter::new(blue_score_ref_iter),
    }
}
