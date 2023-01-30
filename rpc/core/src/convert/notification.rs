use std::sync::Arc;

use crate::{notify::collector::ArcConvert, BlockAddedNotification, NewBlockTemplateNotification, Notification, NotificationMessage};
use consensus_core::notify as consensus_notify;

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&consensus_notify::Notification> for Notification {
    fn from(item: &consensus_notify::Notification) -> Self {
        match item {
            consensus_notify::Notification::BlockAdded(msg) => Notification::BlockAdded(msg.into()),
            consensus_notify::Notification::NewBlockTemplate(msg) => Notification::NewBlockTemplate(msg.into()),
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

impl From<ArcConvert<NotificationMessage>> for Arc<Notification> {
    fn from(item: ArcConvert<NotificationMessage>) -> Self {
        (*item).payload.clone()
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
