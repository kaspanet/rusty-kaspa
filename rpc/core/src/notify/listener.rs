use crate::{
    notify::{
        channel::NotificationChannel,
        events::{EventArray, EventType},
        result::Result,
        utxo_address_set::RpcUtxoAddressSet,
    },
    Notification, NotificationMessage, NotificationReceiver, NotificationSender, NotificationType, RpcAddress,
};
use std::fmt::Debug;
use std::sync::Arc;

pub type ListenerID = u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListenerUtxoNotificationFilterSetting {
    /// Send all changed UTXO events, whatever the address
    All,

    /// Send all changed UTXO events filtered by the address
    FilteredByAddress,
}

/// A listener of [`super::notifier::Notifier`] notifications.
///
/// ### Implementation details
///
/// This struct is not async protected against mutations.
/// It is the responsibility of code using a [Listener] to guard memory
/// before calling toggle.
///
/// Any ListenerSenderSide derived from a [Listener] should also be rebuilt
/// upon relevant mutation by a call to toggle.
#[derive(Debug)]
pub(crate) struct Listener {
    id: u64,
    sender: NotificationSender,
    active_event: EventArray<bool>,
    utxo_addresses: RpcUtxoAddressSet,
}

impl Listener {
    pub(crate) fn new(id: ListenerID, sender: NotificationSender) -> Listener {
        Self { id, sender, active_event: EventArray::default(), utxo_addresses: RpcUtxoAddressSet::new() }
    }

    pub(crate) fn id(&self) -> ListenerID {
        self.id
    }

    /// Has registered for [`EventType`] notifications?
    pub(crate) fn has(&self, event: EventType) -> bool {
        self.active_event[event]
    }

    fn toggle_utxo_addresses(&mut self, utxo_addresses: &[RpcAddress]) -> bool {
        let utxo_addresses = RpcUtxoAddressSet::from_iter(utxo_addresses.iter().cloned());
        if utxo_addresses != self.utxo_addresses {
            self.utxo_addresses = utxo_addresses;
            return true;
        }
        false
    }

    /// Toggle registration for [`NotificationType`] notifications.
    /// Return true if any change occurred in the registration state.
    pub(crate) fn toggle(&mut self, notification_type: NotificationType, active: bool) -> bool {
        let mut changed = false;
        let event: EventType = (&notification_type).into();

        if self.active_event[event] != active {
            self.active_event[event] = active;
            changed = true;
        }

        if let NotificationType::UtxosChanged(ref utxo_addresses) = notification_type {
            changed = self.toggle_utxo_addresses(utxo_addresses);
        }
        changed
    }

    pub(crate) fn close(&mut self) {
        if !self.is_closed() {
            self.sender.close();
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}

/// Contains the receiver side of a listener
#[derive(Debug)]
pub struct ListenerReceiverSide {
    pub id: ListenerID,
    pub recv_channel: NotificationReceiver,
}

// impl From<(u64,NotificationReceiver)> for ListenerReceiverSide {
//     fn from((id,recv_channel): (u64,NotificationReceiver)) -> ListenerReceiverSide {
//         ListenerReceiverSide::new(id, recv_channel)
//     }
// }

impl ListenerReceiverSide {
    pub fn new(id: ListenerID, recv_channel: NotificationReceiver) -> ListenerReceiverSide {
        ListenerReceiverSide { id, recv_channel }
    }
}

// impl From<&Listener> for ListenerReceiverSide {
//     fn from(item: &Listener) -> Self {
//         // Self { id: item.id(), recv_channel: item.sender.receiver() }
//         Self { id: item.id(), recv_channel: item.sender.receiver() }
//     }
// }

#[derive(Debug)]
/// Contains the sender side of a listener
pub(crate) struct ListenerSenderSide {
    // id: ListenerID,
    send_channel: NotificationSender,
    filter: Box<dyn Filter + Send + Sync>,
}

impl ListenerSenderSide {
    pub(crate) fn new(listener: &Listener, sending_changed_utxos: ListenerUtxoNotificationFilterSetting, event: EventType) -> Self {
        match event {
            EventType::UtxosChanged if sending_changed_utxos == ListenerUtxoNotificationFilterSetting::FilteredByAddress => Self {
                send_channel: listener.sender.clone(), //.sender(),
                filter: Box::new(FilterUtxoAddress { utxos_addresses: listener.utxo_addresses.clone() }),
            },
            _ => Self { send_channel: listener.sender.clone(), filter: Box::new(Unfiltered {}) },
        }
    }

    /// Try to send a notification.
    ///
    /// If the notification does not meet requirements (see [`Notification::UtxosChanged`]) returns `Ok(false)`,
    /// otherwise returns `Ok(true)`.
    pub(crate) fn try_send(&self, id: &ListenerID, notification: Arc<Notification>) -> Result<bool> {
        if self.filter.matches(notification.clone()) {
            match self.send_channel.try_send(Arc::new(NotificationMessage::new(*id, notification))) {
                Ok(_) => {
                    return Ok(true);
                }
                Err(err) => {
                    return Err(err.into());
                }
            }
        }
        Ok(false)
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.send_channel.is_closed()
    }
}

trait InnerFilter {
    fn matches(&self, notification: Arc<Notification>) -> bool;
}

trait Filter: InnerFilter + Debug {}

#[derive(Clone, Debug)]
struct Unfiltered;
impl InnerFilter for Unfiltered {
    fn matches(&self, _: Arc<Notification>) -> bool {
        true
    }
}
impl Filter for Unfiltered {}

#[derive(Clone, Debug)]
struct FilterUtxoAddress {
    utxos_addresses: RpcUtxoAddressSet,
}

impl InnerFilter for FilterUtxoAddress {
    fn matches(&self, notification: Arc<Notification>) -> bool {
        if let Notification::UtxosChanged(ref notification) = *notification {
            // TODO: redesign the filter
            // We want to limit the notification contents to the watched addresses only.
            return notification.added.iter().any(|x| self.utxos_addresses.contains(&x.address))
                || notification.removed.iter().any(|x| self.utxos_addresses.contains(&x.address));
        }
        false
    }
}
impl Filter for FilterUtxoAddress {}
