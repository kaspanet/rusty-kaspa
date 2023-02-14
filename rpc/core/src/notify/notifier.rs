use super::{
    broadcaster::Broadcaster,
    collector::DynCollector,
    connection::Connection,
    error::{Error, Result},
    events::{EventArray, EventType},
    listener::{Listener, ListenerId},
    scope::Scope,
    subscriber::{Subscriber, SubscriptionManager},
    subscription::{array::ArrayBuilder, CompoundedSubscription, Mutation},
};
use crate::{api::ops::SubscribeCommand, Notification, RpcResult};
use async_trait::async_trait;
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

/// A notification sender
///
/// Manages a collection of [`Listener`] and, for each one, a set of events to be notified.
/// Actually notify the listeners of incoming events.
#[derive(Debug)]
pub struct Notifier<C>
where
    C: Connection,
{
    inner: Arc<Inner<C>>,
}

impl<C> Notifier<C>
where
    C: Connection,
{
    pub fn new(collector: Option<DynCollector<C>>, subscriber: Option<Subscriber>, broadcasters: usize, name: &'static str) -> Self {
        Self { inner: Arc::new(Inner::new(collector, subscriber, broadcasters, name)) }
    }

    pub fn start(self: Arc<Self>) {
        self.inner.clone().start(self.clone());
    }

    pub fn register_new_listener(&self, connection: C) -> ListenerId {
        self.inner.clone().register_new_listener(connection)
    }

    pub fn unregister_listener(&self, id: ListenerId) -> Result<()> {
        self.inner.unregister_listener(id)
    }

    pub fn execute_subscribe_command(self: Arc<Self>, id: ListenerId, scope: Scope, command: SubscribeCommand) -> Result<()> {
        self.inner.execute_subscribe_command(id, scope, command)
    }

    pub fn start_notify(&self, id: ListenerId, scope: Scope) -> Result<()> {
        self.inner.clone().start_notify(id, scope)
    }

    pub fn notify(self: Arc<Self>, notification: Arc<Notification>) -> Result<()> {
        self.inner.clone().notify(notification)
    }

    pub fn stop_notify(&self, id: ListenerId, scope: Scope) -> Result<()> {
        self.inner.clone().stop_notify(id, scope)
    }

    pub async fn stop(&self) -> Result<()> {
        self.inner.clone().stop().await
    }
}

#[async_trait]
impl<C> SubscriptionManager for Notifier<C>
where
    C: Connection,
{
    async fn start_notify(self: Arc<Self>, id: ListenerId, scope: Scope) -> RpcResult<()> {
        trace!(
            "[Notifier-{}] as subscription manager start sending to listener {} notifications of type {:?}",
            self.inner.name,
            id,
            scope
        );
        self.inner.clone().start_notify(id, scope)?;
        Ok(())
    }

    async fn stop_notify(self: Arc<Self>, id: ListenerId, scope: Scope) -> RpcResult<()> {
        trace!(
            "[Notifier-{}] as subscription manager stop sending to listener {} notifications of type {:?}",
            self.inner.name,
            id,
            scope
        );
        self.inner.clone().stop_notify(id, scope)?;
        Ok(())
    }
}

#[derive(Debug)]
struct Inner<C>
where
    C: Connection,
{
    /// Map of registered listeners
    listeners: Mutex<HashMap<ListenerId, Listener<C>>>,

    /// Compounded subscriptions by event type
    subscriptions: Mutex<EventArray<CompoundedSubscription>>,

    /// Has this notifier been started?
    started: Arc<AtomicBool>,

    /// Channel used to send the notifications to the broadcasters
    notification_channel: Channel<Arc<Notification>>,

    /// Array of notification broadcasters
    broadcasters: Vec<Arc<Broadcaster<C>>>,

    // Collector & Subscriber
    collector: Arc<Option<DynCollector<C>>>,
    subscriber: Arc<Option<Arc<Subscriber>>>,

    /// Name of the notifier
    pub name: &'static str,
}

impl<C> Inner<C>
where
    C: Connection,
{
    fn new(collector: Option<DynCollector<C>>, subscriber: Option<Subscriber>, broadcasters: usize, name: &'static str) -> Self {
        assert!(broadcasters > 0, "a notifier requires a minimum of one broadcaster");
        let subscriber = subscriber.map(Arc::new);
        let notification_channel = Channel::unbounded();
        let broadcasters = (0..broadcasters)
            .into_iter()
            .map(|_| Arc::new(Broadcaster::new(name, notification_channel.receiver.clone())))
            .collect::<Vec<_>>();
        Self {
            listeners: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(ArrayBuilder::compounded()),
            started: Arc::new(AtomicBool::new(false)),
            notification_channel,
            broadcasters,
            collector: Arc::new(collector),
            subscriber: Arc::new(subscriber),
            name,
        }
    }

    fn start(self: Arc<Self>, notifier: Arc<Notifier<C>>) {
        if self.started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            if let Some(ref subscriber) = self.subscriber.clone().as_ref() {
                subscriber.start();
            }
            self.broadcasters.iter().for_each(|x| x.start());
            if let Some(ref collector) = self.collector.clone().as_ref() {
                collector.clone().start(notifier);
            }
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

    pub fn execute_subscribe_command(self: &Arc<Self>, id: ListenerId, scope: Scope, command: SubscribeCommand) -> Result<()> {
        let event: EventType = (&scope).into();
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
                // Report to the parent
                if let Some(mutation) = compound_result {
                    if let Some(ref subscriber) = self.subscriber.clone().as_ref() {
                        let _ = subscriber.mutate(mutation);
                    }
                }
            }
        }
        Ok(())
    }

    fn start_notify(self: Arc<Self>, id: ListenerId, scope: Scope) -> Result<()> {
        self.execute_subscribe_command(id, scope, SubscribeCommand::Start)
    }

    fn notify(self: Arc<Self>, notification: Arc<Notification>) -> Result<()> {
        Ok(self.notification_channel.try_send(notification)?)
    }

    fn stop_notify(self: Arc<Self>, id: ListenerId, scope: Scope) -> Result<()> {
        self.execute_subscribe_command(id, scope, SubscribeCommand::Stop)
    }

    async fn stop(self: Arc<Self>) -> Result<()> {
        if self.started.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            if let Some(ref collector) = self.collector.clone().as_ref() {
                collector.clone().stop().await?;
            }
            let futures = self.broadcasters.iter().map(|x| x.stop());
            join_all(futures).await.into_iter().collect::<std::result::Result<Vec<()>, _>>()?;
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
