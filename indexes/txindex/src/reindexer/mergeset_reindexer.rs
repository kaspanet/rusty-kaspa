use std::sync::Arc;

use crate::stores::acceptance::{BlueScoreRefIter, BlueScoreRefTuple, TxAcceptedIter, TxAcceptedTuple};
use kaspa_consensus_core::Hash;
use kaspa_consensus_core::acceptance_data::MergesetBlockAcceptanceData;
use kaspa_consensus_core::acceptance_data::MergesetIndexType;
use kaspa_consensus_notify::notification::VirtualChainChangedNotification;

pub struct ReindexedVirtualChangedState<TxIter, BlueScoreIter>
where
    TxIter: Iterator<Item = TxAcceptedTuple>,
    BlueScoreIter: Iterator<Item = BlueScoreRefTuple>,
{
    pub sink_hash: Hash,
    pub sink_blue_score: u64,
    pub reindexed_mergeset_state: Vec<ReindexedMergesetState<TxIter, BlueScoreIter>>,
}

pub struct ReindexedMergesetState<TxIter, BlueScoreIter>
where
    TxIter: Iterator<Item = TxAcceptedTuple>,
    BlueScoreIter: Iterator<Item = BlueScoreRefTuple>,
{
    pub tx_iter: TxAcceptedIter<TxIter>,                      // We hold iterators as to not allocate.
    pub blue_score_ref_iter: BlueScoreRefIter<BlueScoreIter>, // We hold iterators as to not allocate.
}

pub fn reindex_virtual_changed_notification<'a>(
    notification: &'a VirtualChainChangedNotification,
) -> ReindexedVirtualChangedState<impl Iterator<Item = TxAcceptedTuple> + 'a, impl Iterator<Item = BlueScoreRefTuple> + 'a> {
    ReindexedVirtualChangedState {
        sink_hash: *notification.added_chain_block_hashes.last().unwrap(),
        sink_blue_score: *notification.added_accepting_blue_scores.last().unwrap(),
        reindexed_mergeset_state: notification
            .added_chain_block_hashes
            .iter()
            .zip(notification.added_chain_blocks_acceptance_data.iter())
            .zip(notification.added_accepting_blue_scores.iter())
            .flat_map(|((accepting_hash, added_acceptance_vec), added_accepting_blue_score)| {
                added_acceptance_vec.iter().enumerate().map(move |(mergeset_index, mbad)| {
                    reindex_mergeset_acceptance_data(
                        accepting_hash,
                        *added_accepting_blue_score,
                        mergeset_index as MergesetIndexType,
                        mbad,
                    )
                })
            })
            .collect(),
    }
}

pub fn reindex_mergeset_acceptance_data<'a>(
    accepting_block_hash: &'a Hash,
    accepting_blue_score: u64,
    mergeset_index: MergesetIndexType,
    mergeset_block_acceptance: &'a MergesetBlockAcceptanceData,
) -> ReindexedMergesetState<impl Iterator<Item = TxAcceptedTuple> + 'a, impl Iterator<Item = BlueScoreRefTuple> + 'a> {
    let txs_iter = mergeset_block_acceptance
        .accepted_transactions
        .iter()
        .map(move |accepted_tx_entry| (accepted_tx_entry.transaction_id, accepting_blue_score, *accepting_block_hash, mergeset_index));

    let blue_score_refs_iter = mergeset_block_acceptance
        .accepted_transactions
        .iter()
        .map(move |accepted_tx_entry| (accepting_blue_score, accepted_tx_entry.transaction_id));

    ReindexedMergesetState { tx_iter: TxAcceptedIter::new(txs_iter), blue_score_ref_iter: BlueScoreRefIter::new(blue_score_refs_iter) }
}

pub fn reindex_mergeset_acceptance_data_many<'a>(
    accepting_block_hashes: &'a [Hash],
    accepting_blue_scores: &'a [u64],
    acceptance_data: &'a [Arc<Vec<MergesetBlockAcceptanceData>>],
) -> impl Iterator<Item = ReindexedMergesetState<impl Iterator<Item = TxAcceptedTuple> + 'a, impl Iterator<Item = BlueScoreRefTuple> + 'a>>
+ 'a {
    accepting_block_hashes.iter().zip(accepting_blue_scores.iter()).zip(acceptance_data.iter()).flat_map(
        |((accepting_hash, accepting_blue_score), mergeset_acceptance)| {
            mergeset_acceptance.iter().enumerate().map(move |(mergeset_index, mbad)| {
                reindex_mergeset_acceptance_data(accepting_hash, *accepting_blue_score, mergeset_index as MergesetIndexType, mbad)
            })
        },
    )
}

// --- tests ---
#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::acceptance_data::AcceptedTxEntry;
    use kaspa_hashes::Hash;
    use std::sync::Arc;

    #[test]
    fn test_reindex_virtual_changed_notification() {
        // Prepare two accepting blocks with one accepted tx each
        let h1 = Hash::from_slice(&[1u8; 32]);
        let h2 = Hash::from_slice(&[2u8; 32]);
        let tx1 = Hash::from_slice(&[3u8; 32]);
        let tx2 = Hash::from_slice(&[4u8; 32]);

        let mbad1 = MergesetBlockAcceptanceData {
            block_hash: h1,
            accepted_transactions: vec![AcceptedTxEntry { transaction_id: tx1, index_within_block: 0 }],
        };
        let mbad2 = MergesetBlockAcceptanceData {
            block_hash: h2,
            accepted_transactions: vec![AcceptedTxEntry { transaction_id: tx2, index_within_block: 0 }],
        };

        let added_hashes = Arc::new(vec![h1, h2]);
        let removed_hashes = Arc::new(vec![]);
        let added_acceptance = Arc::new(vec![Arc::new(vec![mbad1.clone()]), Arc::new(vec![mbad2.clone()])]);
        let added_accepting_blue_scores = Arc::new(vec![10u64, 20u64]);

        let notification =
            VirtualChainChangedNotification::new(added_hashes, removed_hashes, added_acceptance, added_accepting_blue_scores);

        let state = reindex_virtual_changed_notification(&notification);

        // Sink should be the last added block and blue score the last accepting blue score
        assert_eq!(state.sink_hash, h2);
        assert_eq!(state.sink_blue_score, 20u64);

        // We expect two reindexed mergeset states (one per added mergeset entry)
        assert_eq!(state.reindexed_mergeset_state.len(), 2);

        // Consume the reindexed states and assert the tuples produced by the tx and blue-score iterators
        let mut iter = state.reindexed_mergeset_state.into_iter();

        let ms0 = iter.next().unwrap();
        let txs0: Vec<_> = ms0.tx_iter.collect();
        let blue_refs0: Vec<_> = ms0.blue_score_ref_iter.collect();
        assert_eq!(txs0, vec![(tx1, 10u64, h1, 0)]);
        assert_eq!(blue_refs0, vec![(10u64, tx1)]);

        let ms1 = iter.next().unwrap();
        let txs1: Vec<_> = ms1.tx_iter.collect();
        let blue_refs1: Vec<_> = ms1.blue_score_ref_iter.collect();
        assert_eq!(txs1, vec![(tx2, 20u64, h2, 0)]);
        assert_eq!(blue_refs1, vec![(20u64, tx2)]);
    }

    #[test]
    fn test_reindex_mergeset_acceptance_data() {
        // Prepare a mergeset block acceptance with two accepted transactions
        let accepting_hash = Hash::from_slice(&[9u8; 32]);
        let tx_a = Hash::from_slice(&[10u8; 32]);
        let tx_b = Hash::from_slice(&[11u8; 32]);
        let mbad = MergesetBlockAcceptanceData {
            block_hash: accepting_hash,
            accepted_transactions: vec![
                AcceptedTxEntry { transaction_id: tx_a, index_within_block: 0 },
                AcceptedTxEntry { transaction_id: tx_b, index_within_block: 1 },
            ],
        };

        let mergeset_index: MergesetIndexType = 42;
        let accepting_blue_score = 100u64;

        let ms = reindex_mergeset_acceptance_data(&accepting_hash, accepting_blue_score, mergeset_index, &mbad);

        let txs: Vec<_> = ms.tx_iter.collect();
        let blue_refs: Vec<_> = ms.blue_score_ref_iter.collect();

        assert_eq!(
            txs,
            vec![
                (tx_a, accepting_blue_score, accepting_hash, mergeset_index),
                (tx_b, accepting_blue_score, accepting_hash, mergeset_index),
            ]
        );

        assert_eq!(blue_refs, vec![(accepting_blue_score, tx_a), (accepting_blue_score, tx_b)]);
    }

    #[test]
    pub fn test_reindex_mergeset_acceptance_data_many() {
        // Prepare two accepting blocks with one accepted tx each
        let h1 = Hash::from_slice(&[21u8; 32]);
        let h2 = Hash::from_slice(&[22u8; 32]);
        let tx1 = Hash::from_slice(&[23u8; 32]);
        let tx2 = Hash::from_slice(&[24u8; 32]);

        let mbad1 = MergesetBlockAcceptanceData {
            block_hash: h1,
            accepted_transactions: vec![AcceptedTxEntry { transaction_id: tx1, index_within_block: 0 }],
        };
        let mbad2 = MergesetBlockAcceptanceData {
            block_hash: h2,
            accepted_transactions: vec![AcceptedTxEntry { transaction_id: tx2, index_within_block: 0 }],
        };

        let accepting_hashes = vec![h1, h2];
        let accepting_blue_scores = vec![30u64, 40u64];
        let acceptance_data = vec![Arc::new(vec![mbad1.clone()]), Arc::new(vec![mbad2.clone()])];

        let mut iter = reindex_mergeset_acceptance_data_many(&accepting_hashes, &accepting_blue_scores, &acceptance_data);

        let ms0 = iter.next().unwrap();
        let txs0: Vec<_> = ms0.tx_iter.collect();
        let blue_refs0: Vec<_> = ms0.blue_score_ref_iter.collect();
        assert_eq!(txs0, vec![(tx1, 30u64, h1, 0)]);
        assert_eq!(blue_refs0, vec![(30u64, tx1)]);

        let ms1 = iter.next().unwrap();
        let txs1: Vec<_> = ms1.tx_iter.collect();
        let blue_refs1: Vec<_> = ms1.blue_score_ref_iter.collect();
        assert_eq!(txs1, vec![(tx2, 40u64, h2, 0)]);
        assert_eq!(blue_refs1, vec![(40u64, tx2)]);
    }
}
