use crate::models::txindex::{
    BlockAcceptanceOffset, BlockAcceptanceOffsetDiff, MergesetIndexType, TxHashSet, TxOffset, TxOffsetById, TxOffsetDiff,
};
use kaspa_consensus_core::{BlockHashMap, BlockHashSet, HashMapCustomHasher};
use kaspa_consensus_notify::notification::{
    ChainAcceptanceDataPrunedNotification as ConsensusChainAcceptanceDataPrunedNotification,
    VirtualChainChangedNotification as ConsensusVirtualChainChangedNotification,
};
use kaspa_hashes::Hash;
use kaspa_utils::arc::ArcExtensions;

/// Reindexes a [`ConsensusVirtualChainChangedNotification`] to txindex diffs, alongside new source and sink [`Hash`], this includes the calculated [`BlockAcceptanceOffsetDiff`] and [`TxOffsetDiff`].
#[derive(Clone, Debug, Default)]
pub struct TxIndexReindexer {
    pub new_sink: Option<Hash>,
    pub source: Option<Hash>,
    pub block_acceptance_offsets_changes: BlockAcceptanceOffsetDiff,
    pub tx_offset_changes: TxOffsetDiff,
}

impl From<ConsensusVirtualChainChangedNotification> for TxIndexReindexer {
    fn from(vspcc_notification: ConsensusVirtualChainChangedNotification) -> Self {
        let new_sink = match vspcc_notification.added_chain_block_hashes.last() {
            Some(hash) => Some(*hash),
            None => vspcc_notification.removed_chain_block_hashes.last().copied(),
        };

        drop(vspcc_notification.removed_chain_block_hashes); // we do not require this anymore.

        let mut tx_offsets_to_add = TxOffsetById::new();
        let mut tx_offsets_to_remove = TxHashSet::new();
        let mut block_acceptance_offsets_to_add =
            BlockHashMap::<BlockAcceptanceOffset>::with_capacity(vspcc_notification.added_chain_blocks_acceptance_data.len());
        let mut block_acceptance_offsets_to_remove =
            BlockHashSet::with_capacity(vspcc_notification.removed_chain_blocks_acceptance_data.len());

        for (accepting_block_hash, acceptance_data) in vspcc_notification
            .added_chain_block_hashes
            .unwrap_or_clone()
            .into_iter()
            .zip(vspcc_notification.added_chain_blocks_acceptance_data.unwrap_or_clone().into_iter())
        {
            for (i, mergeset) in acceptance_data.unwrap_or_clone().into_iter().enumerate() {
                tx_offsets_to_add.extend(
                    mergeset
                        .accepted_transactions
                        .into_iter()
                        .map(|tx_entry| (tx_entry.transaction_id, TxOffset::new(mergeset.block_hash, tx_entry.index_within_block))),
                );

                block_acceptance_offsets_to_add
                    .insert(mergeset.block_hash, BlockAcceptanceOffset::new(accepting_block_hash, i as MergesetIndexType));
            }
        }

        for acceptance_data in vspcc_notification.removed_chain_blocks_acceptance_data.unwrap_or_clone().into_iter() {
            for mergeset in acceptance_data.unwrap_or_clone().into_iter() {
                tx_offsets_to_remove.extend(
                    mergeset
                        .accepted_transactions
                        .into_iter()
                        .filter(|tx_entry| !tx_offsets_to_add.contains_key(&tx_entry.transaction_id))
                        .map(|tx_entry| tx_entry.transaction_id),
                );

                if !block_acceptance_offsets_to_add.contains_key(&mergeset.block_hash) {
                    block_acceptance_offsets_to_remove.insert(mergeset.block_hash);
                };
            }
        }

        Self {
            new_sink,
            source: None,
            block_acceptance_offsets_changes: BlockAcceptanceOffsetDiff::new(
                block_acceptance_offsets_to_add,
                block_acceptance_offsets_to_remove,
            ),
            tx_offset_changes: TxOffsetDiff::new(tx_offsets_to_add, tx_offsets_to_remove),
        }
    }
}

impl From<ConsensusChainAcceptanceDataPrunedNotification> for TxIndexReindexer {
    fn from(notification: ConsensusChainAcceptanceDataPrunedNotification) -> Self {
        let mut tx_offsets_to_remove = TxHashSet::new();
        let mut block_acceptance_offsets_to_remove =
            BlockHashSet::with_capacity(notification.mergeset_block_acceptance_data_pruned.len());

        for mergeset in notification.mergeset_block_acceptance_data_pruned.unwrap_or_clone().into_iter() {
            tx_offsets_to_remove.extend(mergeset.accepted_transactions.into_iter().map(|tx_entry| tx_entry.transaction_id));
            block_acceptance_offsets_to_remove.insert(mergeset.block_hash);
        }

        Self {
            new_sink: None,
            source: Some(notification.history_root),
            block_acceptance_offsets_changes: BlockAcceptanceOffsetDiff::new(BlockHashMap::new(), block_acceptance_offsets_to_remove),
            tx_offset_changes: TxOffsetDiff::new(TxOffsetById::new(), tx_offsets_to_remove),
        }
    }
}
