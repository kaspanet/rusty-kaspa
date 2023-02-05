use crate::{
    notify::{
        connection::Connection,
        events::{EventArray, EventType},
        result::Result,
        utxo_address_set::RpcUtxoAddressSet,
    },
    Notification, NotificationType, RpcAddress,
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
pub(crate) struct Listener<T>
where
    T: Connection,
{
    id: u64,
    connection: T,
    active_event: EventArray<bool>,
    utxo_addresses: RpcUtxoAddressSet,
}

impl<T> Listener<T>
where
    T: Connection,
{
    pub(crate) fn new(id: ListenerID, connection: T) -> Self {
        Self { id, connection, active_event: EventArray::default(), utxo_addresses: RpcUtxoAddressSet::new() }
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

    pub(crate) fn close(&self) {
        if !self.is_closed() {
            self.connection.close();
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.connection.is_closed()
    }
}

#[derive(Debug)]
/// Contains the sender side of a listener
pub(crate) struct ListenerSenderSide<T>
where
    T: Connection,
{
    connection: T,
    filter: Box<dyn Filter + Send + Sync>,
}

impl<T> ListenerSenderSide<T>
where
    T: Connection,
{
    pub(crate) fn new(listener: &Listener<T>, sending_changed_utxos: ListenerUtxoNotificationFilterSetting, event: EventType) -> Self {
        match event {
            EventType::UtxosChanged if sending_changed_utxos == ListenerUtxoNotificationFilterSetting::FilteredByAddress => Self {
                connection: listener.connection.clone(),
                filter: Box::new(FilterUtxoAddress { utxos_addresses: listener.utxo_addresses.clone() }),
            },
            _ => Self { connection: listener.connection.clone(), filter: Box::new(Unfiltered {}) },
        }
    }

    pub(crate) fn build_utxos_changed_notification(&self, notification: &Arc<Notification>) -> Option<Arc<Notification>> {
        // FIXME: actually build a filtered Notification::UtxosChanged
        match self.filter.matches(notification.clone()) {
            true => Some(notification.clone()),
            false => None,
        }
    }

    /// Try to send a notification.
    ///
    /// If the notification does not meet requirements (see [`Notification::UtxosChanged`]) returns `Ok(false)`,
    /// otherwise returns `Ok(true)`.
    pub(crate) fn try_send(&self, message: T::Message) -> Result<bool> {
        // FIXME: externalize the logic of building a filtered Notification::UtxosChanged
        //if self.filter.matches(notification.clone()) {
        match self.connection.send(message) {
            Ok(_) => Ok(true),
            Err(err) => Err(err.into()),
        }
        //}
        //Ok(false)
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.connection.is_closed()
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
