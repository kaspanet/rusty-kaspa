use crate::stores::inclusion::{DaaScoreRefIter, DaaScoreRefTuple, TxInclusionIter, TxInclusionTuple};
use kaspa_consensus_core::Hash;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::tx::TransactionIndexType;
use kaspa_consensus_notify::notification::BlockAddedNotification;

pub struct ReindexedBlockAddedState<TxIter, DaaScoreIter>
where
    TxIter: Iterator<Item = TxInclusionTuple>,
    DaaScoreIter: Iterator<Item = DaaScoreRefTuple>,
{
    pub block_hash: Hash,
    pub direct_parents: Vec<Hash>,
    pub body: ReindexedBlockBodyState<TxIter, DaaScoreIter>,
}

pub struct ReindexedBlockBodyState<TxIter, DaaScoreIter>
where
    TxIter: Iterator<Item = TxInclusionTuple>,
    DaaScoreIter: Iterator<Item = DaaScoreRefTuple>,
{
    pub tx_iter: TxInclusionIter<TxIter>,                  // We hold iterators as to not re-allocate.
    pub daa_score_ref_iter: DaaScoreRefIter<DaaScoreIter>, // We hold iterators as to not re-allocate.
}

/// Returns the block hash and an iterator over (TransactionId, u64, Hash, TransactionIndexType) for the given BlockAddedNotification.
pub fn reindex_block_added_notification<'a>(
    block_added_notification: &'a BlockAddedNotification,
) -> ReindexedBlockAddedState<impl Iterator<Item = TxInclusionTuple> + 'a, impl Iterator<Item = DaaScoreRefTuple> + 'a> {
    reindex_block(&block_added_notification.block)
}

pub fn reindex_block<'a>(
    block: &'a Block,
) -> ReindexedBlockAddedState<impl Iterator<Item = TxInclusionTuple> + 'a, impl Iterator<Item = DaaScoreRefTuple> + 'a> {
    let block_hash = block.header.hash;
    let daa_score = block.header.daa_score;
    let tx_iter = block.transactions.iter().enumerate().map(move |(index_within_block, transaction)| {
        (transaction.id(), daa_score, block_hash, index_within_block as TransactionIndexType)
    });
    let daa_score_ref_iter = block.transactions.iter().map(move |transaction| (block.header.daa_score, transaction.id()));
    ReindexedBlockAddedState {
        block_hash,
        direct_parents: block.header.direct_parents().to_vec(),
        body: ReindexedBlockBodyState {
            tx_iter: TxInclusionIter::new(tx_iter),
            daa_score_ref_iter: DaaScoreRefIter::new(daa_score_ref_iter),
        },
    }
}

pub fn reindex_blocks<'a>(
    blocks: impl Iterator<Item = &'a Block> + 'a,
) -> impl Iterator<
    Item = ReindexedBlockAddedState<impl Iterator<Item = TxInclusionTuple> + 'a, impl Iterator<Item = DaaScoreRefTuple> + 'a>,
> + 'a {
    blocks.map(|block| reindex_block(block))
}

// --- tests ---
#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus::test_helpers::generate_random_block;
    use kaspa_consensus_notify::notification::BlockAddedNotification;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    #[test]
    fn test_reindex_block() {
        let mut rng = SmallRng::from_entropy();
        let block = generate_random_block(&mut rng, 2, 3, 1, 1);

        let state = reindex_block(&block);
        assert_eq!(state.block_hash, block.header.hash);
        assert_eq!(state.direct_parents, block.header.direct_parents().to_vec());

        let txs: Vec<_> = state.body.tx_iter.collect();
        let daa_refs: Vec<_> = state.body.daa_score_ref_iter.collect();

        let expected_txs: Vec<_> = block
            .transactions
            .iter()
            .enumerate()
            .map(|(i, tx)| (tx.id(), block.header.daa_score, block.header.hash, i as TransactionIndexType))
            .collect();

        let expected_refs: Vec<_> = block.transactions.iter().map(|tx| (block.header.daa_score, tx.id())).collect();

        assert_eq!(txs, expected_txs);
        assert_eq!(daa_refs, expected_refs);
    }

    #[test]
    fn test_reindex_block_added_notification() {
        let mut rng = SmallRng::from_entropy();
        let block = generate_random_block(&mut rng, 1, 2, 1, 1);
        let notification = BlockAddedNotification::new(block.clone());

        let state = reindex_block_added_notification(&notification);
        assert_eq!(state.block_hash, block.header.hash);

        let txs: Vec<_> = state.body.tx_iter.collect();
        assert_eq!(txs.len(), block.transactions.len());
        for (i, tx) in block.transactions.iter().enumerate() {
            assert_eq!(txs[i], (tx.id(), block.header.daa_score, block.header.hash, i as TransactionIndexType));
        }
    }

    #[test]
    fn test_reindex_blocks_iterator() {
        let mut rng = SmallRng::from_entropy();
        let b1 = generate_random_block(&mut rng, 1, 1, 1, 1);
        let b2 = generate_random_block(&mut rng, 1, 2, 1, 1);

        let res: Vec<_> = reindex_blocks(vec![&b1, &b2].into_iter()).collect();
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].block_hash, b1.header.hash);
        assert_eq!(res[1].block_hash, b2.header.hash);
    }
}
