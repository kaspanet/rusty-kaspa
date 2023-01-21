use std::sync::Arc;

use crate::{
    api::notifications::*, notify::collector::ArcConvert, stubs::UtxosChangedNotification, BlockAddedNotification,
    ConsensusNotification, NewBlockTemplateNotification,
};
use consensus_core::notify as consensus_notify;
use utxoindex::notify as utxoindex_notify;

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&consensus_notify::ConsensusNotification> for Notification {
    fn from(item: &consensus_notify::ConsensusNotification) -> Self {
        match item {
            consensus_notify::ConsensusNotification::BlockAdded(msg) => Notification::BlockAdded(msg.into()),
            consensus_notify::ConsensusNotification::NewBlockTemplate(msg) => Notification::NewBlockTemplate(msg.into()),
            consensus_notify::ConsensusNotification::VirtualChangeSet(msg) => Notification::VirtualChangeSet(msg.into()),
            consensus_notify::ConsensusNotification::PruningPointUTXOSetOverride(msg) => {
                Notification::PruningPointUTXOSetOverride(msg.into())
            }
            _ => todo!("match missing notifications"), //TODO: fill missing notifications
        }
    }
}

impl From<&utxoindex_notify::UtxoIndexNotification> for UtxosChangedNotification {
    fn from(item: &consensus_notify::Notification) -> Self {
        match item {
            utxoindex_notify::UtxoIndexNotification::UtxosChanged(msg) => Notification::UtxosChanged(msg.into()),
        }
    }
}

impl From<&consensus_notify::BlockAddedNotification> for BlockAddedNotification {
    fn from(item: &consensus_notify::BlockAddedNotification) -> Self {
        Self { block: (&item.block).into() }
    }
}

impl From<&consensus_notify::NewBlockTemplateNotification> for NewBlockTemplateNotification {
    fn from(_: &consensus_notify::NewBlockTemplateNotification) -> Self {
        Self {}
    }
}

impl From<&consensus_notify::NewBlockTemplateNotification> for NewBlockTemplateNotification {
    fn from(_: &consensus_notify::NewBlockTemplateNotification) -> Self {
        Self {}
    }
}

impl From<&consensus_notify::PruningPointUTXOSetOverrideNotification> for PruningPointUTXOSetOverrideNotification {
    fn from(_: &consensus_notify::PruningPointUTXOSetOverrideNotification) -> Self {
        Self {}
    }
}

impl From<&consensus_notify::VirtualStateChangeNotification> for PruningPointUTXOSetOverrideNotification {
    fn from(_: &consensus_notify::PruningPointUTXOSetOverrideNotification) -> Self {
        Self {}
    }
}

/// Pseudo conversion from Arc<Notification> to Arc<Notification>.
/// This is basically a clone() op.
impl From<ArcConvert<Notification>> for Arc<Notification> {
    fn from(item: ArcConvert<Notification>) -> Self {
        (*item).clone()
    }
}

impl From<ArcConvert<consensus_notify::Notification>> for Arc<Notification> {
    fn from(item: ArcConvert<consensus_notify::Notification>) -> Self {
        Arc::new((&**item).into())
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------
