use std::sync::Arc;

use crate::{notify::collector::rpc_collector::ArcConvert, BlockAddedNotification, NewBlockTemplateNotification, Notification};
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
            _ => todo!(),
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

/// Pseudo conversion from Arc<Notification> to Arc<Notification>.
/// This is basically a clone() op.
impl From<ArcConvert<Notification>> for Arc<Notification> {
    fn from(item: ArcConvert<Notification>) -> Self {
        (*item).clone()
    }
}

impl From<ArcConvert<consensus_notify::ConsensusNotification>> for Arc<Notification> {
    fn from(item: ArcConvert<consensus_notify::ConsensusNotification>) -> Self {
        Arc::new((&**item).into())
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

// ----------------------------------------------------------------------------
// rpc_core to rpc_core
// ----------------------------------------------------------------------------

// ----------------------------------------------------------------------------
// utxoindex_core to rpc_core
// ----------------------------------------------------------------------------

impl TryFrom<&utxoindex_notify::UtxoIndexNotification> for Notification {
    fn try_from(item: &utxoindex_notify::UtxoIndexNotification) -> Self {
        match item {
            utxoindex_notify::UtxoIndexNotification::UtxoChanges(msg) => Notification::UtxosChanged(msg.try_into()?),
            _ => todo!(),
        }
    }
}
