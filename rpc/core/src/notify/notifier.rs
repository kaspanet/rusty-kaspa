use super::{
    collector::DynCollector,
    connection::Connection,
    error::Error,
    events::{EventArray, EventType, EVENT_TYPE_ARRAY},
    listener::{Listener, ListenerID, ListenerSenderSide, ListenerUtxoNotificationFilterSetting, ListenerVariantSet},
    message::{DispatchMessage, SubscribeMessage},
    result::Result,
    subscriber::{Subscriber, SubscriptionManager},
};
use crate::{api::ops::SubscribeCommand, Notification, NotificationType, RpcResult};
use async_channel::{Receiver, Sender};
use async_trait::async_trait;
use kaspa_core::trace;
use kaspa_utils::channel::Channel;
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

/// A notification sender
///
/// Manages a collection of [`Listener`] and, for each one, a set of events to be notified.
/// Actually notify the listeners of incoming events.
#[derive(Debug)]
pub struct Notifier<T>
where
    T: Connection,
{
    inner: Arc<Inner<T>>,
}

impl<T> Notifier<T>
where
    T: Connection,
{
    pub fn new(
        collector: Option<DynCollector<T>>,
        subscriber: Option<Subscriber>,
        sending_changed_utxos: ListenerUtxoNotificationFilterSetting,
        name: &'static str,
    ) -> Self {
        Self { inner: Arc::new(Inner::new(collector, subscriber, sending_changed_utxos, name)) }
    }

    pub fn start(self: Arc<Self>) {
        self.inner.clone().start(self.clone());
    }

    pub fn register_new_listener(&self, connection: T) -> ListenerID {
        self.inner.clone().register_new_listener(connection)
    }

    pub fn unregister_listener(&self, id: ListenerID) -> Result<()> {
        self.inner.clone().unregister_listener(id)
    }

    pub fn execute_subscribe_command(
        self: Arc<Self>,
        id: ListenerID,
        notification_type: NotificationType,
        command: SubscribeCommand,
    ) -> Result<()> {
        self.inner.clone().execute_subscribe_command(id, notification_type, command)
    }

    pub fn start_notify(&self, id: ListenerID, notification_type: NotificationType) -> Result<()> {
        self.inner.clone().start_notify(id, notification_type)
    }

    pub fn notify(self: Arc<Self>, notification: Arc<Notification>) -> Result<()> {
        self.inner.clone().notify(notification)
    }

    pub fn stop_notify(&self, id: ListenerID, notification_type: NotificationType) -> Result<()> {
        self.inner.clone().stop_notify(id, notification_type)
    }

    pub async fn stop(&self) -> Result<()> {
        self.inner.clone().stop().await
    }
}

#[async_trait]
impl<T> SubscriptionManager for Notifier<T>
where
    T: Connection,
{
    async fn start_notify(self: Arc<Self>, id: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        trace!(
            "[Notifier-{}] as subscription manager start sending to listener {} notifications of type {:?}",
            self.inner.name,
            id,
            notification_type
        );
        self.inner.clone().start_notify(id, notification_type)?;
        Ok(())
    }

    async fn stop_notify(self: Arc<Self>, id: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        trace!(
            "[Notifier-{}] as subscription manager stop sending to listener {} notifications of type {:?}",
            self.inner.name,
            id,
            notification_type
        );
        self.inner.clone().stop_notify(id, notification_type)?;
        Ok(())
    }
}

#[derive(Debug)]
struct Inner<T>
where
    T: Connection,
{
    /// Map of registered listeners
    listeners: Arc<Mutex<HashMap<ListenerID, Listener<T>>>>,

    /// Has this notifier been started?
    is_started: Arc<AtomicBool>,

    /// Dispatcher channels by event type
    dispatcher_channel: EventArray<Channel<DispatchMessage<T>>>,
    dispatcher_shutdown_listener: Arc<Mutex<EventArray<Option<triggered::Listener>>>>,

    // Collector & Subscriber
    collector: Arc<Option<DynCollector<T>>>,
    subscriber: Arc<Option<Arc<Subscriber>>>,

    /// How to handle UtxoChanged notifications
    sending_changed_utxos: ListenerUtxoNotificationFilterSetting,

    /// Name of the notifier
    pub name: &'static str,
}

impl<T> Inner<T>
where
    T: Connection,
{
    fn new(
        collector: Option<DynCollector<T>>,
        subscriber: Option<Subscriber>,
        sending_changed_utxos: ListenerUtxoNotificationFilterSetting,
        name: &'static str,
    ) -> Self {
        let subscriber = subscriber.map(Arc::new);
        Self {
            listeners: Arc::new(Mutex::new(HashMap::new())),
            is_started: Arc::new(AtomicBool::new(false)),
            dispatcher_channel: EventArray::default(),
            dispatcher_shutdown_listener: Arc::new(Mutex::new(EventArray::default())),
            collector: Arc::new(collector),
            subscriber: Arc::new(subscriber),
            sending_changed_utxos,
            name,
        }
    }

    fn start(self: Arc<Self>, notifier: Arc<Notifier<T>>) {
        if self.is_started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            if let Some(ref subscriber) = self.subscriber.clone().as_ref() {
                subscriber.clone().start();
            }
            for event in EVENT_TYPE_ARRAY.into_iter() {
                let (shutdown_trigger, shutdown_listener) = triggered::trigger();
                let mut dispatcher_shutdown_listener = self.dispatcher_shutdown_listener.lock().unwrap();
                dispatcher_shutdown_listener[event] = Some(shutdown_listener);
                self.spawn_dispatcher_task(event, shutdown_trigger, self.dispatcher_channel[event].receiver());
            }
            if let Some(ref collector) = self.collector.clone().as_ref() {
                collector.clone().start(notifier);
            }
            trace!("[Notifier-{}] started", self.name);
        } else {
            trace!("[Notifier-{}] start ignored since already started", self.name);
        }
    }

    /// Launch a dispatcher task for an event type.
    ///
    /// ### Implementation note
    ///
    /// The separation by event type allows to keep an internal map
    /// with all listeners willing to receive notification of the
    /// corresponding type. The dispatcher receives and executes messages
    /// instructing to modify the map. This happens without blocking
    /// the whole notifier.
    fn spawn_dispatcher_task(
        &self,
        event: EventType,
        shutdown_trigger: triggered::Trigger,
        dispatch_rx: Receiver<DispatchMessage<T>>,
    ) {
        // Feedback
        let send_subscriber = self.subscriber.clone().as_ref().as_ref().map(|x| x.sender());
        let has_subscriber = self.subscriber.clone().as_ref().is_some();

        let sending_changed_utxos = self.sending_changed_utxos;
        let name: &'static str = self.name;

        workflow_core::task::spawn(async move {
            trace!("[Notifier-{}] dispatcher_task starting for notification type {:?}", name, event);

            fn send_subscribe_message(send_subscriber: Sender<SubscribeMessage>, message: SubscribeMessage, name: &'static str) {
                trace!("[Notifier-{name}] dispatcher_task send subscribe message: {:?}", message);
                match send_subscriber.try_send(message) {
                    Ok(_) => {}
                    Err(err) => {
                        trace!("[Notifier-{name}] sending subscribe message error: {:?}", err);
                    }
                }
            }

            // This holds the map of all active listeners by message variant for the event type
            let mut listeners: ListenerVariantSet<T> = ListenerVariantSet::new();

            // TODO: feed the listeners map with pre-existing self.listeners having event active
            // This is necessary for the correct handling of repeating start/stop cycles.

            // We will send subscribe messages for all dispatch messages if event is a filtered UtxosChanged.
            // Otherwise, subscribe message is only sent when needed by the execution of the dispatch message.
            let report_all_changes =
                event == EventType::UtxosChanged && sending_changed_utxos == ListenerUtxoNotificationFilterSetting::FilteredByAddress;

            let mut need_subscribe: bool = false;
            loop {
                // If needed, send subscribe message based on listeners map being empty or not
                if need_subscribe && has_subscriber {
                    if listeners.len() > 0 {
                        // TODO: handle actual utxo address set

                        send_subscribe_message(
                            send_subscriber.as_ref().unwrap().clone(),
                            SubscribeMessage::StartEvent(event.into()),
                            name,
                        );
                    } else {
                        send_subscribe_message(
                            send_subscriber.as_ref().unwrap().clone(),
                            SubscribeMessage::StopEvent(event.into()),
                            name,
                        );
                    }
                }
                let dispatch = dispatch_rx.recv().await.unwrap();

                match dispatch {
                    DispatchMessage::Send(ref notification) => {
                        // Create a store for closed listeners to be removed from the map
                        let mut purge: Vec<ListenerID> = Vec::new();

                        // Broadcast the notification to all listeners
                        match event {
                            // For UtxosChanged notifications, build a filtered notification for every listener
                            EventType::UtxosChanged => {
                                for (variant, listener_set) in listeners.iter() {
                                    for (id, listener) in listener_set.iter() {
                                        if let Some(listener_notification) = listener.build_utxos_changed_notification(notification) {
                                            let message = T::into_message(&listener_notification, variant);
                                            match listener.try_send(message) {
                                                Ok(_) => {
                                                    trace!("[Notifier-{name}] dispatcher_task sent notification {notification} to listener {id}");
                                                }
                                                Err(_) => {
                                                    if listener.is_closed() {
                                                        trace!("[Notifier-{name}] dispatcher_task could not send a notification to listener {id} because it is closed - removing it");
                                                        purge.push(*id);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // For all other notifications, broadcast the same message to all listeners
                            _ => {
                                for (variant, listener_set) in listeners.iter() {
                                    let message = T::into_message(notification, variant);
                                    for (id, listener) in listener_set.iter() {
                                        match listener.try_send(message.clone()) {
                                            Ok(_) => {
                                                trace!("[Notifier-{name}] dispatcher_task sent notification {notification} to listener {id}");
                                            }
                                            Err(_) => {
                                                if listener.is_closed() {
                                                    trace!("[Notifier-{name}] dispatcher_task could not send a notification to listener {id} because it is closed - removing it");
                                                    purge.push(*id);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Feedback needed if purge will empty listeners or if reporting any change
                        need_subscribe = (!purge.is_empty() && (purge.len() == listeners.len())) || report_all_changes;

                        // Remove closed listeners
                        for id in purge {
                            listeners.remove(&id);
                        }
                    }

                    DispatchMessage::AddListener(id, listener) => {
                        // Subscription needed if a first listener is added or if reporting any change
                        need_subscribe = listeners.len() == 0 || report_all_changes;

                        // We don't care whether this is an insertion or a replacement
                        listeners.insert(listener.variant(), id, listener.clone());
                    }

                    DispatchMessage::RemoveListener(id) => {
                        listeners.remove(&id);

                        // Feedback needed if no more listeners are present or if reporting any change
                        need_subscribe = listeners.len() == 0 || report_all_changes;
                    }

                    DispatchMessage::Shutdown => {
                        break;
                    }
                }
            }
            shutdown_trigger.trigger();
            trace!("[Notifier-{name}] dispatcher_task exiting for notification type {:?}", event);
        });
    }

    fn register_new_listener(self: Arc<Self>, connection: T) -> ListenerID {
        let mut listeners = self.listeners.lock().unwrap();
        loop {
            let id = u64::from_le_bytes(rand::random::<[u8; 8]>());

            // This is very unlikely to happen but still, check for duplicates
            if let Entry::Vacant(e) = listeners.entry(id) {
                let listener = Listener::new(id, connection);
                e.insert(listener);
                return id;
            }
        }
    }

    fn unregister_listener(self: Arc<Self>, id: ListenerID) -> Result<()> {
        let mut listeners = self.listeners.lock().unwrap();
        if let Some(listener) = listeners.remove(&id) {
            drop(listeners);
            let active_events: Vec<EventType> = EVENT_TYPE_ARRAY.into_iter().filter(|event| listener.has(*event)).collect();
            for event in active_events.iter() {
                self.clone().stop_notify(listener.id(), (*event).into())?;
            }
            listener.close();
        }
        Ok(())
    }

    pub fn execute_subscribe_command(
        self: Arc<Self>,
        id: ListenerID,
        notification_type: NotificationType,
        command: SubscribeCommand,
    ) -> Result<()> {
        match command {
            SubscribeCommand::Start => self.start_notify(id, notification_type),
            SubscribeCommand::Stop => self.stop_notify(id, notification_type),
        }
    }

    fn start_notify(self: Arc<Self>, id: ListenerID, notification_type: NotificationType) -> Result<()> {
        let event: EventType = (&notification_type).into();
        let mut listeners = self.listeners.lock().unwrap();
        if let Some(listener) = listeners.get_mut(&id) {
            trace!("[Notifier-{}] start notifying to {id} about {:?}", self.name, notification_type);

            // Any mutation in the listener will trigger a dispatch of a brand new ListenerSenderSide
            // eventually creating or replacing this listener in the matching dispatcher.

            if listener.toggle(notification_type, true) {
                let listener_sender_side = ListenerSenderSide::new(listener, self.sending_changed_utxos, event);
                let msg = DispatchMessage::AddListener(listener.id(), Arc::new(listener_sender_side));
                self.clone().try_send_dispatch(event, msg)?;
            }
        }
        Ok(())
    }

    fn notify(self: Arc<Self>, notification: Arc<Notification>) -> Result<()> {
        let event: EventType = notification.as_ref().into();
        let msg = DispatchMessage::Send(notification);
        self.try_send_dispatch(event, msg)?;
        Ok(())
    }

    fn stop_notify(self: Arc<Self>, id: ListenerID, notification_type: NotificationType) -> Result<()> {
        let event: EventType = (&notification_type).into();
        let mut listeners = self.listeners.lock().unwrap();
        if let Some(listener) = listeners.get_mut(&id) {
            if listener.toggle(notification_type.clone(), false) {
                trace!("[Notifier-{}] stop notifying to {id} about {:?}", self.name, notification_type);
                let msg = DispatchMessage::RemoveListener(listener.id());
                self.clone().try_send_dispatch(event, msg)?;
            }
        }
        Ok(())
    }

    fn try_send_dispatch(self: Arc<Self>, event: EventType, msg: DispatchMessage<T>) -> Result<()> {
        self.dispatcher_channel[event].sender().try_send(msg)?;
        Ok(())
    }

    async fn stop_dispatcher_task(self: Arc<Self>) -> Result<()> {
        let mut result: Result<()> = Ok(());
        for event in EVENT_TYPE_ARRAY.into_iter() {
            match self.clone().try_send_dispatch(event, DispatchMessage::Shutdown) {
                Ok(_) => {
                    let shutdown_listener: triggered::Listener;
                    {
                        let mut dispatcher_shutdown_listener = self.dispatcher_shutdown_listener.lock().unwrap();
                        shutdown_listener = dispatcher_shutdown_listener[event].take().unwrap();
                    }
                    shutdown_listener.await;
                }
                Err(err) => result = Err(err),
            }
        }
        result
    }

    async fn stop(self: Arc<Self>) -> Result<()> {
        if self.is_started.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            if let Some(ref collector) = self.collector.clone().as_ref() {
                collector.clone().stop().await?;
            }
            self.clone().stop_dispatcher_task().await?;
            if let Some(ref subscriber) = self.subscriber.clone().as_ref() {
                subscriber.clone().stop().await?;
            }
        } else {
            trace!("[Notifier-{}] stop ignored since already stopped", self.name);
            return Err(Error::AlreadyStoppedError);
        }
        trace!("[Notifier-{}] stopped", self.name);
        Ok(())
    }
}
