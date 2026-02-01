use crate::stores::acceptance::{BlueScoreRefIter, BlueScoreRefTuple, TxAcceptedIter, TxAcceptedTuple};
use kaspa_consensus_core::acceptance_data::MergesetBlockAcceptanceData;
use kaspa_consensus_core::acceptance_data::MergesetIndexType;
use kaspa_consensus_core::Hash;
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
