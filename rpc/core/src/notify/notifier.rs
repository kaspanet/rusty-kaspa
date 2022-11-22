use super::{
    channel::NotificationChannel,
    collector::DynCollector,
    events::{EventArray, EventType, EVENT_TYPE_ARRAY},
    listener::{Listener, ListenerID, ListenerReceiverSide, ListenerSenderSide, SendingChangedUtxo},
    message::{DispatchMessage, SubscribeMessage},
    result::Result,
    subscriber::{Subscriber, SubscriptionManager},
};
use crate::{api::ops::SubscribeCommand, Notification, NotificationType, RpcResult};
use ahash::AHashMap;
use async_std::channel::{Receiver, Sender};
use async_trait::async_trait;
use kaspa_utils::channel::Channel;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

/// A notification sender
///
/// Manage a collection of [Listener] and, for each one, a set of events to be notified.
/// Actually notify the listeners of incoming events.
#[derive(Debug)]
pub struct Notifier {
    inner: Arc<Inner>,
}

impl Notifier {
    pub fn new(collector: Option<DynCollector>, subscriber: Option<Subscriber>, sending_changed_utxos: SendingChangedUtxo) -> Self {
        Self { inner: Arc::new(Inner::new(collector, subscriber, sending_changed_utxos)) }
    }

    pub fn start(self: Arc<Self>) {
        self.inner.clone().start(self.clone());
    }

    pub fn register_new_listener(&self, channel: Option<NotificationChannel>) -> ListenerReceiverSide {
        self.inner.clone().register_new_listener(channel)
    }

    pub fn unregister_listener(&self, id: ListenerID) -> Result<()> {
        self.inner.clone().unregister_listener(id)
    }

    pub fn execute_notify_command(
        self: Arc<Self>,
        id: ListenerID,
        notification_type: NotificationType,
        command: SubscribeCommand,
    ) -> Result<()> {
        self.inner.clone().execute_notify_command(id, notification_type, command)
    }

    pub fn start_notify(&self, id: ListenerID, notification_type: NotificationType) -> Result<()> {
        println!("[Notifier] start sending to listener {0} notifications of type {1:?}", id, notification_type);
        self.inner.clone().start_notify(id, notification_type)
    }

    pub fn notify(self: Arc<Self>, notification: Arc<Notification>) -> Result<()> {
        self.inner.clone().notify(notification)
    }

    pub fn stop_notify(&self, id: ListenerID, notification_type: NotificationType) -> Result<()> {
        println!("[Notifier] stop sending to listener {0} notifications of type {1:?}", id, notification_type);
        self.inner.clone().stop_notify(id, notification_type)
    }

    pub async fn stop(&self) -> Result<()> {
        self.inner.clone().stop().await
    }
}

#[async_trait]
impl SubscriptionManager for Notifier {
    async fn start_notify(self: Arc<Self>, id: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        println!(
            "[Notifier] as subscription manager start sending to listener {0} notifications of type {1:?}",
            id, notification_type
        );
        self.inner.clone().start_notify(id, notification_type)?;
        Ok(())
    }

    async fn stop_notify(self: Arc<Self>, id: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        println!("[Notifier] as subscription manager stop sending to listener {0} notifications of type {1:?}", id, notification_type);
        self.inner.clone().stop_notify(id, notification_type)?;
        Ok(())
    }
}

#[derive(Debug)]
struct Inner {
    /// Map of registered listeners
    listeners: Arc<Mutex<AHashMap<ListenerID, Listener>>>,

    /// Dispatcher channels by event type
    dispatcher_channel: EventArray<Channel<DispatchMessage>>,
    dispatcher_shutdown_listener: Arc<Mutex<EventArray<Option<triggered::Listener>>>>,
    dispatcher_is_running: EventArray<Arc<AtomicBool>>,

    /// Collector & Subscriber
    collector: Arc<Option<DynCollector>>,
    subscriber: Arc<Option<Arc<Subscriber>>>,

    /// How to handle UtxoChanged notifications
    sending_changed_utxos: SendingChangedUtxo,
}

impl Inner {
    fn new(collector: Option<DynCollector>, subscriber: Option<Subscriber>, sending_changed_utxos: SendingChangedUtxo) -> Self {
        let subscriber = subscriber.map(Arc::new);
        Self {
            listeners: Arc::new(Mutex::new(AHashMap::new())),
            dispatcher_channel: EventArray::default(),
            dispatcher_shutdown_listener: Arc::new(Mutex::new(EventArray::default())),
            dispatcher_is_running: EventArray::default(),
            collector: Arc::new(collector),
            subscriber: Arc::new(subscriber),
            sending_changed_utxos,
        }
    }

    fn start(self: Arc<Self>, notifier: Arc<Notifier>) {
        if let Some(ref subscriber) = self.subscriber.clone().as_ref() {
            subscriber.clone().start();
        }
        for event in EVENT_TYPE_ARRAY.into_iter() {
            if !self.clone().dispatcher_is_running[event].load(Ordering::SeqCst) {
                let (shutdown_trigger, shutdown_listener) = triggered::trigger();
                let mut dispatcher_shutdown_listener = self.dispatcher_shutdown_listener.lock().unwrap();
                dispatcher_shutdown_listener[event] = Some(shutdown_listener);
                self.dispatch_task(event, shutdown_trigger, self.dispatcher_channel[event].receiver());
            }
        }
        if let Some(ref collector) = self.collector.clone().as_ref() {
            collector.clone().start(notifier);
        }
    }

    /// Launch a dispatch task for an event type.
    ///
    /// Implementation note:
    /// The separation by event type allows to keep an internal map
    /// with all listeners willing to receive notification of the
    /// corresponding type. The dispatcher receives and execute messages
    /// instructing to modify the map. This happens without blocking
    /// the whole notifier.
    fn dispatch_task(&self, event: EventType, shutdown_trigger: triggered::Trigger, dispatch_rx: Receiver<DispatchMessage>) {
        let dispatcher_is_running = self.dispatcher_is_running[event].clone();
        dispatcher_is_running.store(true, Ordering::SeqCst);

        // Feedback
        let send_subscriber = self.subscriber.clone().as_ref().as_ref().map(|x| x.sender());
        let has_subscriber = self.subscriber.clone().as_ref().is_some();

        let sending_changed_utxos = self.sending_changed_utxos;

        // This holds the map of all active listeners for the event type
        let mut listeners: AHashMap<ListenerID, Arc<ListenerSenderSide>> = AHashMap::new();

        // TODO: feed the listeners map with pre-existing self.listeners having event active
        // This is necessary for the correct handling of repeating start/stop cycles.

        workflow_core::task::spawn(async move {
            println!("[Notifier] dispatch_task spawned");

            fn send_subscribe_message(send_subscriber: Sender<SubscribeMessage>, message: SubscribeMessage) {
                println!("[Notifier] dispatch_task send subscribe message: {:?}", message);
                match send_subscriber.try_send(message) {
                    Ok(_) => {}
                    Err(err) => {
                        println!("[Notifier] sending subscribe message error: {:?}", err);
                    }
                }
            }

            // We will send subscribe messages for all dispatch messages if event is a filtered UtxosChanged.
            // Otherwise, subscribe message is only sent when needed by the execution of the dispatche message.
            let report_all_changes =
                event == EventType::UtxosChanged && sending_changed_utxos == SendingChangedUtxo::FilteredByAddress;

            let mut need_subscribe: bool = false;
            loop {
                // If needed, send subscribe message based on listeners map being empty or not
                if need_subscribe && has_subscriber {
                    if listeners.len() > 0 {
                        // TODO: handle actual utxo addresse set

                        send_subscribe_message(send_subscriber.as_ref().unwrap().clone(), SubscribeMessage::StartEvent(event.into()));
                    } else {
                        send_subscribe_message(send_subscriber.as_ref().unwrap().clone(), SubscribeMessage::StopEvent(event.into()));
                    }
                }
                let dispatch = dispatch_rx.recv().await.unwrap();

                match dispatch {
                    DispatchMessage::Send(ref notification) => {
                        // Create a store for closed listeners to be removed from the map
                        let mut purge: Vec<ListenerID> = Vec::new();

                        // Broadcast the notification to all listeners
                        for (id, listener) in listeners.iter() {
                            match listener.try_send(notification.clone()) {
                                Ok(_) => {}
                                Err(_) => {
                                    if listener.is_closed() {
                                        purge.push(*id);
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
                        listeners.insert(id, listener.clone());
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
            dispatcher_is_running.store(false, Ordering::SeqCst);
            shutdown_trigger.trigger();
        });
    }

    fn register_new_listener(self: Arc<Self>, channel: Option<NotificationChannel>) -> ListenerReceiverSide {
        let mut listeners = self.listeners.lock().unwrap();
        loop {
            let id = u64::from_le_bytes(rand::random::<[u8; 8]>());

            // This is very unlikely to happen but still, check for duplicates
            if !listeners.contains_key(&id) {
                let listener = Listener::new(id, channel);
                let registration: ListenerReceiverSide = (&listener).into();
                listeners.insert(id, listener);
                return registration;
            }
        }
    }

    fn unregister_listener(self: Arc<Self>, id: ListenerID) -> Result<()> {
        let mut listeners = self.listeners.lock().unwrap();
        if let Some(mut listener) = listeners.remove(&id) {
            drop(listeners);
            let active_events: Vec<EventType> = EVENT_TYPE_ARRAY.into_iter().filter(|event| listener.has(*event)).collect();
            for event in active_events.iter() {
                self.clone().stop_notify(listener.id(), (*event).into())?;
            }
            listener.close();
        }
        Ok(())
    }

    pub fn execute_notify_command(
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
            println!("[Notifier] start notify to {0} about {1:?}", id, notification_type);

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
            println!("[Notifier] stop notify to {0} about {1:?}", id, notification_type);

            if listener.toggle(notification_type, false) {
                let msg = DispatchMessage::RemoveListener(listener.id());
                self.clone().try_send_dispatch(event, msg)?;
            }
        }
        Ok(())
    }

    fn try_send_dispatch(self: Arc<Self>, event: EventType, msg: DispatchMessage) -> Result<()> {
        self.dispatcher_channel[event].sender().try_send(msg)?;
        Ok(())
    }

    async fn stop_dispatch(self: Arc<Self>) -> Result<()> {
        let mut result: Result<()> = Ok(());
        for event in EVENT_TYPE_ARRAY.into_iter() {
            if self.dispatcher_is_running[event].load(Ordering::SeqCst) {
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
        }
        result
    }

    async fn stop(self: Arc<Self>) -> Result<()> {
        if let Some(ref collector) = self.collector.clone().as_ref() {
            collector.clone().stop().await?;
        }
        self.clone().stop_dispatch().await?;
        if let Some(ref subscriber) = self.subscriber.clone().as_ref() {
            subscriber.clone().stop().await?;
        }
        Ok(())
    }
}
