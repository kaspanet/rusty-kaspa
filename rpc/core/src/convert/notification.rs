use crate::{
    convert::utxo::utxo_set_into_rpc, BlockAddedNotification, FinalityConflictNotification, FinalityConflictResolvedNotification,
    NewBlockTemplateNotification, Notification, PruningPointUtxoSetOverrideNotification, SinkBlueScoreChangedNotification,
    UtxosChangedNotification, VirtualChainChangedNotification, VirtualDaaScoreChangedNotification,
};
use kaspa_consensus_notify::notification as consensus_notify;
use kaspa_index_core::notification as index_notify;
use std::sync::Arc;

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<consensus_notify::Notification> for Notification {
    fn from(item: consensus_notify::Notification) -> Self {
        (&item).into()
    }
}

impl From<&consensus_notify::Notification> for Notification {
    fn from(item: &consensus_notify::Notification) -> Self {
        match item {
            consensus_notify::Notification::BlockAdded(msg) => Notification::BlockAdded(msg.into()),
            consensus_notify::Notification::VirtualChainChanged(msg) => Notification::VirtualChainChanged(msg.into()),
            consensus_notify::Notification::FinalityConflict(msg) => Notification::FinalityConflict(msg.into()),
            consensus_notify::Notification::FinalityConflictResolved(msg) => Notification::FinalityConflictResolved(msg.into()),
            consensus_notify::Notification::UtxosChanged(msg) => Notification::UtxosChanged(msg.into()),
            consensus_notify::Notification::SinkBlueScoreChanged(msg) => Notification::SinkBlueScoreChanged(msg.into()),
            consensus_notify::Notification::VirtualDaaScoreChanged(msg) => Notification::VirtualDaaScoreChanged(msg.into()),
            consensus_notify::Notification::PruningPointUtxoSetOverride(msg) => Notification::PruningPointUtxoSetOverride(msg.into()),
            consensus_notify::Notification::NewBlockTemplate(msg) => Notification::NewBlockTemplate(msg.into()),
        }
    }
}

impl From<&consensus_notify::BlockAddedNotification> for BlockAddedNotification {
    fn from(item: &consensus_notify::BlockAddedNotification) -> Self {
        Self { block: Arc::new((&item.block).into()) }
    }
}

impl From<&consensus_notify::VirtualChainChangedNotification> for VirtualChainChangedNotification {
    fn from(item: &consensus_notify::VirtualChainChangedNotification) -> Self {
        // TODO: solve the format discrepancy of `accepted_transaction_ids`
        Self {
            removed_chain_block_hashes: item.removed_chain_block_hashes.clone(),
            added_chain_block_hashes: item.added_chain_block_hashes.clone(),
            accepted_transaction_ids: Arc::new(vec![]),
        }
    }
}

impl From<&consensus_notify::FinalityConflictNotification> for FinalityConflictNotification {
    fn from(item: &consensus_notify::FinalityConflictNotification) -> Self {
        Self { violating_block_hash: item.violating_block_hash }
    }
}

impl From<&consensus_notify::FinalityConflictResolvedNotification> for FinalityConflictResolvedNotification {
    fn from(item: &consensus_notify::FinalityConflictResolvedNotification) -> Self {
        Self { finality_block_hash: item.finality_block_hash }
    }
}

impl From<&consensus_notify::UtxosChangedNotification> for UtxosChangedNotification {
    fn from(_: &consensus_notify::UtxosChangedNotification) -> Self {
        // TODO: investigate if this conversion is possible
        UtxosChangedNotification::default()
    }
}

impl From<&consensus_notify::SinkBlueScoreChangedNotification> for SinkBlueScoreChangedNotification {
    fn from(item: &consensus_notify::SinkBlueScoreChangedNotification) -> Self {
        Self { sink_blue_score: item.sink_blue_score }
    }
}

impl From<&consensus_notify::VirtualDaaScoreChangedNotification> for VirtualDaaScoreChangedNotification {
    fn from(item: &consensus_notify::VirtualDaaScoreChangedNotification) -> Self {
        Self { virtual_daa_score: item.virtual_daa_score }
    }
}

impl From<&consensus_notify::PruningPointUtxoSetOverrideNotification> for PruningPointUtxoSetOverrideNotification {
    fn from(_: &consensus_notify::PruningPointUtxoSetOverrideNotification) -> Self {
        Self {}
    }
}

impl From<&consensus_notify::NewBlockTemplateNotification> for NewBlockTemplateNotification {
    fn from(_: &consensus_notify::NewBlockTemplateNotification) -> Self {
        Self {}
    }
}

// ----------------------------------------------------------------------------
// index to rpc_core
// ----------------------------------------------------------------------------

impl From<index_notify::Notification> for Notification {
    fn from(item: index_notify::Notification) -> Self {
        (&item).into()
    }
}

impl From<&index_notify::Notification> for Notification {
    fn from(item: &index_notify::Notification) -> Self {
        match item {
            index_notify::Notification::UtxosChanged(msg) => Notification::UtxosChanged(msg.into()),
            index_notify::Notification::PruningPointUtxoSetOverride(msg) => Notification::PruningPointUtxoSetOverride(msg.into()),
        }
    }
}

impl From<&index_notify::PruningPointUtxoSetOverrideNotification> for PruningPointUtxoSetOverrideNotification {
    fn from(_: &index_notify::PruningPointUtxoSetOverrideNotification) -> Self {
        Self {}
    }
}

impl From<&index_notify::UtxosChangedNotification> for UtxosChangedNotification {
    // This is not intended to be ever called because no address prefix is available.
    // Use kaspa_rpc_service::converter::index::IndexConverter instead.
    fn from(item: &index_notify::UtxosChangedNotification) -> Self {
        Self { added: Arc::new(utxo_set_into_rpc(&item.added, None)), removed: Arc::new(utxo_set_into_rpc(&item.removed, None)) }
    }
}
