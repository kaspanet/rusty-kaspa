use super::{
    broadcaster::Broadcaster,
    collector::DynCollector,
    connection::Connection,
    error::{Error, Result},
    events::{EventArray, EventSwitches, EventType},
    listener::{Listener, ListenerId},
    notification::Notification,
    scope::Scope,
    subscriber::{Subscriber, SubscriptionManager},
    subscription::{array::ArrayBuilder, Command, CompoundedSubscription, Mutation},
};
use async_trait::async_trait;
use core::fmt::Debug;
use futures::future::join_all;
use kaspa_core::trace;
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use workflow_core::channel::Channel;

pub trait Notify<N>: Send + Sync + Debug
where
    N: Notification,
{
    fn notify(&self, notification: N) -> Result<()>;
}

pub type DynNotify<N> = Arc<dyn Notify<N>>;

/// A Notifier is a notification broadcaster that manages a collection of [`Listener`]s and, for each one,
/// a set of subscriptions to notifications by event type.
///
/// A Notifier may own some [`DynCollector`]s which collect incoming notifications and relay them
/// to their owner. The notification sources of the collectors should be considered as the "parents" in
/// the notification DAG.
///
/// A Notifier may own some [`Subscriber`]s which report the subscription needs of their owner's listeners
/// to the "parents" in the notification DAG.
///
/// A notifier broadcasts its incoming notifications to its listeners.
///
/// A notifier is build with a specific set of enabled event types (see `enabled_events`). All disabled
/// event types are ignored by it. It is however possible to manually subscribe to a disabled scope and
/// thus have a custom made collector of the notifier receive notifications of the disabled scope,
/// allowing some handling of the notification into the collector before it gets dropped by the notifier.
#[derive(Debug)]
pub struct Notifier<N, C>
where
    N: Notification,
    C: Connection<Notification = N>,
{
    inner: Arc<Inner<N, C>>,
}

impl<N, C> Notifier<N, C>
where
    N: Notification,
    C: Connection<Notification = N>,
{
    pub fn new(
        enabled_events: EventSwitches,
        collectors: Vec<DynCollector<N>>,
        subscribers: Vec<Arc<Subscriber>>,
        broadcasters: usize,
        name: &'static str,
    ) -> Self {
        Self { inner: Arc::new(Inner::new(enabled_events, collectors, subscribers, broadcasters, name)) }
    }

    pub fn start(self: Arc<Self>) {
        self.inner.clone().start(self.clone());
    }

    pub fn register_new_listener(&self, connection: C) -> ListenerId {
        self.inner.clone().register_new_listener(connection)
    }

    pub fn try_start_notify(&self, id: ListenerId, scope: Scope) -> Result<()> {
        self.inner.clone().start_notify(id, scope)
    }

    pub fn try_execute_subscribe_command(&self, id: ListenerId, scope: Scope, command: Command) -> Result<()> {
        self.inner.clone().execute_subscribe_command(id, scope, command)
    }

    pub fn try_stop_notify(&self, id: ListenerId, scope: Scope) -> Result<()> {
        self.inner.clone().stop_notify(id, scope)
    }

    pub fn unregister_listener(&self, id: ListenerId) -> Result<()> {
        self.inner.unregister_listener(id)
    }

    pub async fn stop(&self) -> Result<()> {
        self.inner.clone().stop().await
    }
}

impl<N, C> Notify<N> for Notifier<N, C>
where
    N: Notification,
    C: Connection<Notification = N>,
{
    fn notify(&self, notification: N) -> Result<()> {
        self.inner.notify(notification)
    }
}

#[async_trait]
impl<N, C> SubscriptionManager for Notifier<N, C>
where
    N: Notification,
    C: Connection<Notification = N>,
{
    async fn start_notify(&self, id: ListenerId, scope: Scope) -> Result<()> {
        trace!("[Notifier-{}] start sending to listener {} notifications of scope {:?}", self.inner.name, id, scope);
        self.inner.start_notify(id, scope)?;
        Ok(())
    }

    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> Result<()> {
        trace!("[Notifier-{}] stop sending to listener {} notifications of scope {:?}", self.inner.name, id, scope);
        self.inner.stop_notify(id, scope)?;
        Ok(())
    }
}

#[derive(Debug)]
struct Inner<N, C>
where
    N: Notification,
    C: Connection,
{
    /// Event types this notifier is configured to accept, broadcast and subscribe to
    enabled_events: EventSwitches,

    /// Map of registered listeners
    listeners: Mutex<HashMap<ListenerId, Listener<C>>>,

    /// Compounded subscriptions by event type
    subscriptions: Mutex<EventArray<CompoundedSubscription>>,

    /// Has this notifier been started?
    started: Arc<AtomicBool>,

    /// Channel used to send the notifications to the broadcasters
    notification_channel: Channel<N>,

    /// Array of notification broadcasters
    broadcasters: Vec<Arc<Broadcaster<N, C>>>,

    /// Collectors
    collectors: Vec<DynCollector<N>>,

    /// Subscribers
    subscribers: Vec<Arc<Subscriber>>,

    /// Name of the notifier, used in logs
    pub name: &'static str,
}

impl<N, C> Inner<N, C>
where
    N: Notification,
    C: Connection<Notification = N>,
{
    fn new(
        enabled_events: EventSwitches,
        collectors: Vec<DynCollector<N>>,
        subscribers: Vec<Arc<Subscriber>>,
        broadcasters: usize,
        name: &'static str,
    ) -> Self {
        assert!(broadcasters > 0, "a notifier requires a minimum of one broadcaster");
        let notification_channel = Channel::unbounded();
        let broadcasters = (0..broadcasters)
            .into_iter()
            .map(|_| Arc::new(Broadcaster::new(name, notification_channel.receiver.clone())))
            .collect::<Vec<_>>();
        Self {
            enabled_events,
            listeners: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(ArrayBuilder::compounded()),
            started: Arc::new(AtomicBool::new(false)),
            notification_channel,
            broadcasters,
            collectors,
            subscribers,
            name,
        }
    }

    fn start(&self, notifier: Arc<Notifier<N, C>>) {
        if self.started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            self.subscribers.iter().for_each(|x| x.start());
            self.collectors.iter().for_each(|x| x.clone().start(notifier.clone()));
            self.broadcasters.iter().for_each(|x| x.start());
            trace!("[Notifier-{}] started", self.name);
        } else {
            trace!("[Notifier-{}] start ignored since already started", self.name);
        }
    }

    fn register_new_listener(self: &Arc<Self>, connection: C) -> ListenerId {
        let mut listeners = self.listeners.lock().unwrap();
        loop {
            let id = u64::from_le_bytes(rand::random::<[u8; 8]>());

            // This is very unlikely to happen but still, check for duplicates
            if let Entry::Vacant(e) = listeners.entry(id) {
                let listener = Listener::new(connection);
                e.insert(listener);
                return id;
            }
        }
    }

    fn unregister_listener(self: &Arc<Self>, id: ListenerId) -> Result<()> {
        // Cancel all remaining subscriptions
        let mut subscriptions = vec![];
        if let Some(listener) = self.listeners.lock().unwrap().get(&id) {
            subscriptions.extend(listener.subscriptions.iter().filter_map(|subscription| {
                if subscription.active() {
                    Some(subscription.scope())
                } else {
                    None
                }
            }));
            listener.close();
        }
        subscriptions.drain(..).for_each(|scope| {
            let _ = self.clone().stop_notify(id, scope);
        });
        // Remove listener
        self.listeners.lock().unwrap().remove(&id);
        Ok(())
    }

    pub fn execute_subscribe_command(&self, id: ListenerId, scope: Scope, command: Command) -> Result<()> {
        let event: EventType = (&scope).into();
        if self.enabled_events[event] {
            let mut listeners = self.listeners.lock().unwrap();
            if let Some(listener) = listeners.get_mut(&id) {
                let mut subscriptions = self.subscriptions.lock().unwrap();
                trace!("[Notifier-{}] {command} notifying to {id} about {scope:?}", self.name);
                if let Some(mutations) = listener.mutate(Mutation::new(command, scope)) {
                    // Update broadcasters
                    let subscription = listener.subscriptions[event].clone_arc();
                    self.broadcasters.iter().for_each(|broadcaster| {
                        let _ = broadcaster.register(subscription.clone(), id, listener.connection());
                    });
                    // Compound mutations
                    let mut compound_result = None;
                    for mutation in mutations {
                        compound_result = subscriptions[event].compound(mutation);
                    }
                    // Report to the parents
                    if let Some(mutation) = compound_result {
                        self.subscribers.iter().for_each(|x| {
                            let _ = x.mutate(mutation.clone());
                        });
                    }
                }
            }
        } else {
            return Err(Error::EventTypeDisabled);
        }
        Ok(())
    }

    fn start_notify(&self, id: ListenerId, scope: Scope) -> Result<()> {
        self.execute_subscribe_command(id, scope, Command::Start)
    }

    fn notify(&self, notification: N) -> Result<()> {
        if self.enabled_events[notification.event_type()] {
            self.notification_channel.try_send(notification)?;
        }
        Ok(())
    }

    fn stop_notify(&self, id: ListenerId, scope: Scope) -> Result<()> {
        self.execute_subscribe_command(id, scope, Command::Stop)
    }

    async fn stop(self: Arc<Self>) -> Result<()> {
        if self.started.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            join_all(self.collectors.iter().map(|x| x.clone().stop()))
                .await
                .into_iter()
                .collect::<std::result::Result<Vec<()>, _>>()?;
            join_all(self.broadcasters.iter().map(|x| x.stop())).await.into_iter().collect::<std::result::Result<Vec<()>, _>>()?;
            join_all(self.subscribers.iter().map(|x| x.stop())).await.into_iter().collect::<std::result::Result<Vec<()>, _>>()?;
        } else {
            trace!("[Notifier-{}] stop ignored since already stopped", self.name);
            return Err(Error::AlreadyStoppedError);
        }
        trace!("[Notifier-{}] stopped", self.name);
        Ok(())
    }
}

// #[cfg(test)]
pub mod test_helpers {
    use super::*;
    use async_channel::Sender;

    #[derive(Debug)]
    pub struct NotifyMock<N>
    where
        N: Notification,
    {
        sender: Sender<N>,
    }

    impl<N> NotifyMock<N>
    where
        N: Notification,
    {
        pub fn new(sender: Sender<N>) -> Self {
            Self { sender }
        }
    }

    impl<N> Notify<N> for NotifyMock<N>
    where
        N: Notification,
    {
        fn notify(&self, notification: N) -> Result<()> {
            Ok(self.sender.try_send(notification)?)
        }
    }
}

// TODO: tests
