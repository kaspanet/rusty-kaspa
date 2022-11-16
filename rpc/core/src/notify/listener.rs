use std::fmt::Debug;
use std::sync::Arc;

use super::channel::NotificationChannel;
use super::events::{EventArray, EventType};
use super::result::Result;
use super::utxo_address_map::RpcUtxoAddressMap;
use crate::stubs::RpcUtxoAddress;
use crate::{Notification, NotificationReceiver, NotificationSender, NotificationType};

// TODO: consider the use of a newtype instead
pub type ListenerID = u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendingChangedUtxo {
    /// Send all changed UTXO events, whatever the address
    All,

    /// Send all changed UTXO events filtered by the address
    FilteredByAddress,
}

/// A listener of [`super::notifier::Notifier`] notifications.
///
/// ### Implementation details
///
/// This struct is not asyn protected against mutations.
/// It is the responsability of code using a [Listener] to guard memory
/// before calling toggle.
///
/// Any ListenerSenderSide derived from a [Listener] should also be rebuilt
/// upon relevant mutation by a call to toggle.
#[derive(Debug)]
pub(crate) struct Listener {
    id: u64,
    channel: NotificationChannel,
    active_event: EventArray<bool>,
    utxo_addresses: RpcUtxoAddressMap,
}

impl Listener {
    pub(crate) fn new(id: ListenerID, channel: Option<NotificationChannel>) -> Listener {
        let channel = channel.unwrap_or_default();
        Self { id, channel, active_event: EventArray::default(), utxo_addresses: RpcUtxoAddressMap::new() }
    }

    pub(crate) fn id(&self) -> ListenerID {
        self.id
    }

    /// Has registered for [`EventType`] notifications?
    pub(crate) fn has(&self, event: EventType) -> bool {
        self.active_event[event]
    }

    fn toggle_utxo_addresses(&mut self, utxo_addresses: &Vec<RpcUtxoAddress>) -> bool {
        let utxo_addresses: RpcUtxoAddressMap = utxo_addresses.into();
        if utxo_addresses != self.utxo_addresses {
            self.utxo_addresses = utxo_addresses;
            return true;
        }
        false
    }

    /// Toggle registration for [`NotificationType`] notifications.
    /// Return true if any change occured in the registration state.
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
    filter: Box<dyn Filter + Send + Sync>,
}

impl ListenerSenderSide {
    pub(crate) fn new(listener: &Listener, sending_changed_utxos: SendingChangedUtxo, event: EventType) -> Self {
        match event {
            EventType::UtxosChanged if sending_changed_utxos == SendingChangedUtxo::FilteredByAddress => Self {
                send_channel: listener.channel.sender(),
                filter: Box::new(FilterUtxoAddress { utxos_addresses: listener.utxo_addresses.clone() }),
            },
            _ => Self { send_channel: listener.channel.sender(), filter: Box::new(Unfiltered {}) },
        }
    }

    /// Try to send a notification.
    ///
    /// If the notification does not meet requirements (see [`Notification::UtxosChanged`]) returns `Ok(false)`,
    /// otherwise returns `Ok(true)`.
    pub(crate) fn try_send(&self, notification: Arc<Notification>) -> Result<bool> {
        if self.filter.filter(notification.clone()) {
            match self.send_channel.try_send(notification) {
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
    fn filter(&self, notification: Arc<Notification>) -> bool;
}

trait Filter: InnerFilter + Debug {}

#[derive(Clone, Debug)]
struct Unfiltered;
impl InnerFilter for Unfiltered {
    fn filter(&self, _: Arc<Notification>) -> bool {
        true
    }
}
impl Filter for Unfiltered {}

#[derive(Clone, Debug)]
struct FilterUtxoAddress {
    utxos_addresses: RpcUtxoAddressMap,
}

impl InnerFilter for FilterUtxoAddress {
    fn filter(&self, notification: Arc<Notification>) -> bool {
        if let Notification::UtxosChanged(ref notification) = *notification {
            return self.utxos_addresses.contains_key(&notification.utxo_address);
        }
        false
    }
}
impl Filter for FilterUtxoAddress {}
