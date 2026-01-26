use crate::stores::inclusion::{BlueScoreRefIter, BlueScoreRefTuple, TxInclusionIter, TxInclusionTuple};
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::tx::TransactionIndexType;
use kaspa_consensus_core::Hash;
use kaspa_consensus_notify::notification::BlockAddedNotification;

pub struct ReindexedBlockAddedState<TxIter, BlueScoreIter>
where
    TxIter: Iterator<Item = TxInclusionTuple>,
    BlueScoreIter: Iterator<Item = BlueScoreRefTuple>,
{
    pub block_hash: Hash,
    pub direct_parents: Vec<Hash>,
    pub body: ReindexedBlockBodyState<TxIter, BlueScoreIter>,
}

pub struct ReindexedBlockBodyState<TxIter, BlueScoreIter>
where
    TxIter: Iterator<Item = TxInclusionTuple>,
    BlueScoreIter: Iterator<Item = BlueScoreRefTuple>,
{
    pub tx_iter: TxInclusionIter<TxIter>, // We hold iterators as to not re-allocate.
    pub blue_score_ref_iter: BlueScoreRefIter<BlueScoreIter>, // We hold iterators as to not re-allocate.
}

/// Returns the block hash and an iterator over (TransactionId, u64, Hash, TransactionIndexType) for the given BlockAddedNotification.
pub fn reindex_block_added_notification<'a>(
    block_added_notification: &'a BlockAddedNotification,
) -> ReindexedBlockAddedState<impl Iterator<Item = TxInclusionTuple> + 'a, impl Iterator<Item = BlueScoreRefTuple> + 'a> {
    reindex_block(&block_added_notification.block)
}

pub fn reindex_block<'a>(
    block: &'a Block,
) -> ReindexedBlockAddedState<impl Iterator<Item = TxInclusionTuple> + 'a, impl Iterator<Item = BlueScoreRefTuple> + 'a> {
    let block_hash = block.header.hash;
    let blue_score = block.header.blue_score;
    let tx_iter = block
        .transactions
        .iter()
        .enumerate()
        .map(move |(index_within_block, tx)| (tx.id(), blue_score, block_hash, index_within_block as TransactionIndexType));
    let blue_score_ref_iter = block.transactions.iter().map(move |tx| (block.header.blue_score, tx.id()));
    ReindexedBlockAddedState {
        block_hash,
        direct_parents: block.header.direct_parents().to_vec(),
        body: ReindexedBlockBodyState {
            tx_iter: TxInclusionIter::new(tx_iter),
            blue_score_ref_iter: BlueScoreRefIter::new(blue_score_ref_iter),
        },
    }
}

pub fn reindex_blocks<'a>(
    blocks: impl Iterator<Item = &'a Block> + 'a,
) -> impl Iterator<
    Item = ReindexedBlockAddedState<impl Iterator<Item = TxInclusionTuple> + 'a, impl Iterator<Item = BlueScoreRefTuple> + 'a>,
> + 'a {
    blocks.map(|block| reindex_block(block))
}
