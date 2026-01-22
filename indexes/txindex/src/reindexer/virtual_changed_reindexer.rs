use crate::stores::{BlueScoreRefIter, BlueScoreRefTuple, RefType, TxAcceptedIter, TxAcceptedTuple};
use kaspa_consensus_core::Hash;
use kaspa_consensus_core::{acceptance_data::MergesetIndexType, tx::TransactionId};
use kaspa_consensus_notify::notification::VirtualChainChangedNotification;

pub struct ReindexedVirtualChangedState<TxIter, BlueScoreIter>
where
    TxIter: Iterator<Item = TxAcceptedTuple>,
    BlueScoreIter: Iterator<Item = BlueScoreRefTuple>,
{
    pub sink_hash: Hash,
    pub sink_blue_score: u64,
    pub tx_iter: TxAcceptedIter<TxIter>,
    pub blue_score_ref_iter: BlueScoreRefIter<BlueScoreIter>,
}

pub fn reindex_virtual_changed_notification<'a>(
    notification: &'a VirtualChainChangedNotification,
) -> ReindexedVirtualChangedState<impl Iterator<Item = TxAcceptedTuple> + 'a, impl Iterator<Item = BlueScoreRefTuple> + 'a> {
    let sink_hash = notification.added_chain_block_hashes.first().expect("expected a sink");
    let sink_blue_score = notification
        .added_chain_blocks_acceptance_data
        .first()
        .expect("expected acceptance data for sink")
        .first()
        .expect("expected at least one mergeset block acceptance data")
        .accepting_blue_score;
    let txs_iter =
        notification.added_chain_block_hashes.iter().zip(notification.added_chain_blocks_acceptance_data.iter()).enumerate().flat_map(
            move |(mergeset_index, (block_hash, acceptance_data))| {
                acceptance_data.iter().flat_map(move |mergeset_block_acceptance_data| {
                    mergeset_block_acceptance_data.accepted_transactions.iter().map(move |accepted_tx_entry| {
                        (
                            accepted_tx_entry.transaction_id,
                            mergeset_block_acceptance_data.accepting_blue_score,
                            *block_hash,
                            mergeset_index as MergesetIndexType,
                        )
                    })
                })
            },
        );
    let blue_score_refs_iter = notification.added_chain_blocks_acceptance_data.iter().flat_map(move |acceptance_data| {
        acceptance_data.iter().flat_map(move |mergeset_block_acceptance_data| {
            mergeset_block_acceptance_data.accepted_transactions.iter().map(move |accepted_tx_entry| {
                (mergeset_block_acceptance_data.accepting_blue_score, RefType::Acceptance, accepted_tx_entry.transaction_id)
            })
        })
    });
    ReindexedVirtualChangedState {
        sink_hash: *sink_hash,
        sink_blue_score,
        tx_iter: TxAcceptedIter::new(txs_iter),
        blue_score_ref_iter: BlueScoreRefIter::new(blue_score_refs_iter),
    }
}
