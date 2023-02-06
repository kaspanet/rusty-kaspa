use std::fmt::Debug;
use std::sync::Arc;

use ahash::{AHashMap, AHashSet};

use crate::{
    notify::{
        channel::NotificationChannel,
        events::{EventArray, EventType},
        result::Result,
        utxo_address_set::RpcAddressSet,
        listener::{filters::UtxosChangedFilter, settings::ListnerSettings, transformers::ListenerTransformers},
    },
    RpcAddress, UtxosChangedNotification,
};
use crate::{Notification, UtxoChangedNotificationTypeModification, NotificationReceiver, NotificationSender, NotificationType};

pub type ListenerID = u64;

/// A listener of [`super::notifier::Notifier`] notifications.
///
/// ### Implementation details
///
/// This struct is not async protected against mutations.
/// It is the responsability of code using a [Listener] to guard memory
/// before calling toggle.
///
/// Any ListenerSenderSide derived from a [Listener] should also be rebuilt
/// upon relevant mutation by a call to toggle.
#[derive(Debug)]
pub(crate) struct Listener {
    id: u64,
    channel: NotificationChannel,
    settings: Arc<ListnerPolicy>,
}

impl Listener {
    pub(crate) fn new(id: ListenerID, channel: Option<NotificationChannel>) -> Listener {
        let channel = channel.unwrap_or_default();
        let policy = ListnerPolicy::new(EventArray::default(), None);
        Self { id, channel, Arc::new(policy) }
    }

    pub(crate) fn id(&self) -> ListenerID {
        self.id
    }

    /// Has registered for [`EventType`] notifications?
    pub(crate) fn has(&self, event: EventType) -> bool {
        self.settings.active_events[event]
    }

    pub(crate) fn close(&mut self) {
        if !self.is_closed() {
            self.channel.close();
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.channel.is_closed()
    }
}

/// Contains the receiver side of a listener
#[derive(Debug)]
pub struct ListenerReceiverSide {
    pub id: ListenerID,
    pub recv_channel: NotificationReceiver,
}

impl From<&Listener> for ListenerReceiverSide {
    fn from(item: &Listener) -> Self {
        Self { id: item.id(), recv_channel: item.channel.receiver() }
    }
}

#[derive(Debug)]
/// Contains the sender side of a listener
pub(crate) struct ListenerSenderSide {
    send_channel: NotificationSender,
    settings: Arc<ListnerPolicy>, //note: don't ever change settings via accessing this directly. Settings should only ever be changed only via relevent notificationTypes!.  
}

impl ListenerSenderSide {
    pub(crate) fn new(listener: Listener, event: EventType) -> Self {
        match event {
            default => Self { send_channel: listener.channel.sender(), settings: ListenerSettings::default() },
        }
    }

    /// Try to send a notification.
    ///
    /// If the notification does not meet requirements (for example, see [`Notification::UtxosChanged`]) return `Ok(false)`,
    /// otherwise return `Ok(true)`.
    pub(crate) fn try_send(&self, notification: Arc<Notification>) -> Result<bool> {
        match self.transform(notification) {
            Some(notification) => match self.send_channel.try_send(notification) {
                Ok(_) => {
                    return Ok(true);
                }
                Err(err) => {
                    return Err(err.into());
                }
            },
            None => return Ok(false),
        }
    }

    pub fn settings(&self) -> ListnerPolicy {
        &self.settings
    }

    /// Manipulate (i.e. process, filter, transform, or void) incomming notifications according to the listener's settings, see [`ListenerSettings`], 
    /// for all available settings. 
    pub fn transform(&self, notification: Arc<Notification>) -> Option<Arc<Notification>> {
        match notification {
            Notification::UtxosChanged(_) => return self,
            default => return Some(Notfication),
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.send_channel.is_closed()
    }
}
