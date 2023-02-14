use std::ops::Deref;

use crate::{BlockAddedNotification, NewBlockTemplateNotification, Notification};
use event_processor::notify as event_notify;

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<event_notify::Notification> for Notification {
    fn from(item: event_notify::Notification) -> Self {
        match item {
            event_notify::Notification::BlockAdded(msg) => Notification::BlockAdded((*msg.deref()).clone().into()),
            event_notify::Notification::NewBlockTemplate(msg) => Notification::NewBlockTemplate(msg.into()),
            _ => todo!(),
        }
    }
}

impl From<event_notify::BlockAddedNotification> for BlockAddedNotification {
    fn from(item: event_notify::BlockAddedNotification) -> Self {
        Self { block: (&item.block).into() }
    }
}

impl From<event_notify::NewBlockTemplateNotification> for NewBlockTemplateNotification {
    fn from(_: event_notify::NewBlockTemplateNotification) -> Self {
        Self {}
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------
