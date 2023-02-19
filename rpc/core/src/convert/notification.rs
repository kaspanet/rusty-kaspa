use crate::{BlockAddedNotification, NewBlockTemplateNotification, Notification};
use event_processor::notify as consensus_notify;
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
            consensus_notify::Notification::BlockAdded(msg) => Notification::BlockAdded((&**msg).into()),
            consensus_notify::Notification::NewBlockTemplate(msg) => Notification::NewBlockTemplate(msg.into()),
            _ => todo!(),
        }
    }
}

impl From<&consensus_notify::BlockAddedNotification> for BlockAddedNotification {
    fn from(item: &consensus_notify::BlockAddedNotification) -> Self {
        Self { block: Arc::new((&item.block).into()) }
    }
}

impl From<&consensus_notify::NewBlockTemplateNotification> for NewBlockTemplateNotification {
    fn from(_: &consensus_notify::NewBlockTemplateNotification) -> Self {
        Self {}
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------
