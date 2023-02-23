use crate::{
    utxo::utxo_set_into_rpc, BlockAddedNotification, FinalityConflictNotification, FinalityConflictResolvedNotification,
    NewBlockTemplateNotification, Notification, PruningPointUtxoSetOverrideNotification, UtxosChangedNotification,
    VirtualDaaScoreChangedNotification, VirtualSelectedParentBlueScoreChangedNotification,
    VirtualSelectedParentChainChangedNotification,
};
use consensus_notify::notification as consensus_notify;
use event_processor::notify as event_processor_notify;
use kaspa_index_processor::notification as index_notify;
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
            consensus_notify::Notification::VirtualSelectedParentChainChanged(msg) => {
                Notification::VirtualSelectedParentChainChanged(msg.into())
            }
            consensus_notify::Notification::FinalityConflict(msg) => Notification::FinalityConflict(msg.into()),
            consensus_notify::Notification::FinalityConflictResolved(msg) => Notification::FinalityConflictResolved(msg.into()),
            consensus_notify::Notification::UtxosChanged(msg) => Notification::UtxosChanged(msg.into()),
            consensus_notify::Notification::VirtualSelectedParentBlueScoreChanged(msg) => {
                Notification::VirtualSelectedParentBlueScoreChanged(msg.into())
            }
            consensus_notify::Notification::VirtualDaaScoreChanged(msg) => Notification::VirtualDaaScoreChanged(msg.into()),
            consensus_notify::Notification::PruningPointUtxoSetOverride(msg) => Notification::PruningPointUtxoSetOverride(msg.into()),
            consensus_notify::Notification::NewBlockTemplate(msg) => Notification::NewBlockTemplate(msg.into()),
        }
    }
}

impl From<&consensus_notify::BlockAddedNotification> for BlockAddedNotification {
    fn from(item: &consensus_notify::BlockAddedNotification) -> Self {
        Self { block: Arc::new((&*item.block).into()) }
    }
}

impl From<&consensus_notify::VirtualSelectedParentChainChangedNotification> for VirtualSelectedParentChainChangedNotification {
    fn from(item: &consensus_notify::VirtualSelectedParentChainChangedNotification) -> Self {
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
        Self { violating_block_hash: item.violating_block_hash.clone() }
    }
}

impl From<&consensus_notify::FinalityConflictResolvedNotification> for FinalityConflictResolvedNotification {
    fn from(item: &consensus_notify::FinalityConflictResolvedNotification) -> Self {
        Self { finality_block_hash: item.finality_block_hash.clone() }
    }
}

impl From<&consensus_notify::UtxosChangedNotification> for UtxosChangedNotification {
    fn from(_: &consensus_notify::UtxosChangedNotification) -> Self {
        // TODO: investigate if this conversion is possible
        UtxosChangedNotification::default()
    }
}

impl From<&consensus_notify::VirtualSelectedParentBlueScoreChangedNotification> for VirtualSelectedParentBlueScoreChangedNotification {
    fn from(item: &consensus_notify::VirtualSelectedParentBlueScoreChangedNotification) -> Self {
        Self { virtual_selected_parent_blue_score: item.virtual_selected_parent_blue_score }
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
    fn from(item: &index_notify::UtxosChangedNotification) -> Self {
        Self { added: Arc::new(utxo_set_into_rpc(&item.added)), removed: Arc::new(utxo_set_into_rpc(&item.removed)) }
    }
}

// ----------------------------------------------------------------------------
// event_processor to rpc_core
// ----------------------------------------------------------------------------

impl From<event_processor_notify::Notification> for Notification {
    fn from(item: event_processor_notify::Notification) -> Self {
        (&item).into()
    }
}

impl From<&event_processor_notify::Notification> for Notification {
    fn from(item: &event_processor_notify::Notification) -> Self {
        match item {
            event_processor_notify::Notification::BlockAdded(msg) => Notification::BlockAdded((&**msg).into()),
            event_processor_notify::Notification::NewBlockTemplate(msg) => Notification::NewBlockTemplate(msg.into()),
            _ => todo!(),
        }
    }
}

impl From<&event_processor_notify::BlockAddedNotification> for BlockAddedNotification {
    fn from(item: &event_processor_notify::BlockAddedNotification) -> Self {
        Self { block: Arc::new((&item.block).into()) }
    }
}

impl From<&event_processor_notify::NewBlockTemplateNotification> for NewBlockTemplateNotification {
    fn from(_: &event_processor_notify::NewBlockTemplateNotification) -> Self {
        Self {}
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------
