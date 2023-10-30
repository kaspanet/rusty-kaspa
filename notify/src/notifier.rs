use crate::events::EVENT_TYPE_ARRAY;

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
use async_channel::Sender;
use async_trait::async_trait;
use core::fmt::Debug;
use futures::future::join_all;
use kaspa_core::{debug, trace};
use parking_lot::Mutex;
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
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

// pub trait Registrar<N>: Send + Sync + Debug
// where
//     N: Notification,
// {
// }

// pub type DynRegistrar<N> = Arc<dyn Registrar<N>>;

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
        name: &'static str,
        enabled_events: EventSwitches,
        collectors: Vec<DynCollector<N>>,
        subscribers: Vec<Arc<Subscriber>>,
        broadcasters: usize,
    ) -> Self {
        Self::with_sync(name, enabled_events, collectors, subscribers, broadcasters, None)
    }

    pub fn with_sync(
        name: &'static str,
        enabled_events: EventSwitches,
        collectors: Vec<DynCollector<N>>,
        subscribers: Vec<Arc<Subscriber>>,
        broadcasters: usize,
        _sync: Option<Sender<()>>,
    ) -> Self {
        Self { inner: Arc::new(Inner::new(name, enabled_events, collectors, subscribers, broadcasters, _sync)) }
    }

    pub fn start(self: Arc<Self>) {
        self.inner.clone().start(self.clone());
    }

    pub fn register_new_listener(&self, connection: C) -> ListenerId {
        self.inner.clone().register_new_listener(connection)
    }

    /// Resend the compounded subscription state of the notifier to its subscribers (its parents).
    ///
    /// The typical use case is a RPC client reconnecting to a server and resending the compounded subscriptions of its listeners.
    pub fn try_renew_subscriptions(&self) -> Result<()> {
        self.inner.clone().renew_subscriptions()
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

    pub async fn join(&self) -> Result<()> {
        self.inner.clone().join().await
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
        trace!("[Notifier {}] start sending to listener {} notifications of scope {:?}", self.inner.name, id, scope);
        self.inner.start_notify(id, scope)?;
        Ok(())
    }

    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> Result<()> {
        trace!("[Notifier {}] stop sending to listener {} notifications of scope {:?}", self.inner.name, id, scope);
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

    /// Sync channel, for handling of messages in predictable sequence; exclusively intended for tests.
    _sync: Option<Sender<()>>,
}

impl<N, C> Inner<N, C>
where
    N: Notification,
    C: Connection<Notification = N>,
{
    fn new(
        name: &'static str,
        enabled_events: EventSwitches,
        collectors: Vec<DynCollector<N>>,
        subscribers: Vec<Arc<Subscriber>>,
        broadcasters: usize,
        _sync: Option<Sender<()>>,
    ) -> Self {
        assert!(broadcasters > 0, "a notifier requires a minimum of one broadcaster");
        let notification_channel = Channel::unbounded();
        let broadcasters = (0..broadcasters)
            .map(|_| Arc::new(Broadcaster::new(name, notification_channel.receiver.clone(), _sync.clone())))
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
            _sync,
        }
    }

    fn start(&self, notifier: Arc<Notifier<N, C>>) {
        if self.started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            trace!("[Notifier {}] starting", self.name);
            self.subscribers.iter().for_each(|x| x.start());
            self.collectors.iter().for_each(|x| x.clone().start(notifier.clone()));
            self.broadcasters.iter().for_each(|x| x.start());
            trace!("[Notifier {}] started", self.name);
        } else {
            trace!("[Notifier {}] start ignored since already started", self.name);
        }
    }

    fn register_new_listener(self: &Arc<Self>, connection: C) -> ListenerId {
        let mut listeners = self.listeners.lock();
        loop {
            let id = u64::from_le_bytes(rand::random::<[u8; 8]>());

            // This is very unlikely to happen but still, check for duplicates
            if let Entry::Vacant(e) = listeners.entry(id) {
                trace!("[Notifier {}] registering listener {id}", self.name);
                let listener = Listener::new(connection);
                e.insert(listener);
                return id;
            }
        }
    }

    fn unregister_listener(self: &Arc<Self>, id: ListenerId) -> Result<()> {
        // Cancel all remaining subscriptions
        let mut subscriptions = vec![];
        if let Some(listener) = self.listeners.lock().get(&id) {
            trace!("[Notifier {}] unregistering listener {id}", self.name);
            subscriptions.extend(listener.subscriptions.iter().filter_map(|subscription| {
                if subscription.active() {
                    Some(subscription.scope())
                } else {
                    None
                }
            }));
        } else {
            trace!("[Notifier {}] unregistering listener {id} error: unknown listener id", self.name);
        }
        subscriptions.drain(..).for_each(|scope| {
            let _ = self.clone().stop_notify(id, scope);
        });

        // Remove & close listener
        if let Some(listener) = self.listeners.lock().remove(&id) {
            trace!("[Notifier {}] closing listener {id}", self.name);
            listener.close();
        }
        Ok(())
    }

    pub fn execute_subscribe_command(&self, id: ListenerId, scope: Scope, command: Command) -> Result<()> {
        let event: EventType = (&scope).into();
        if self.enabled_events[event] {
            let mut listeners = self.listeners.lock();
            if let Some(listener) = listeners.get_mut(&id) {
                let mut subscriptions = self.subscriptions.lock();
                trace!("[Notifier {}] {command} notifying to {id} about {scope:?}", self.name);
                if let Some(mutations) = listener.mutate(Mutation::new(command, scope.clone())) {
                    trace!("[Notifier {}] {command} notifying to {id} about {scope:?} involves mutations {mutations:?}", self.name);
                    // Update broadcasters
                    let subscription = listener.subscriptions[event].clone_arc();
                    self.broadcasters
                        .iter()
                        .try_for_each(|broadcaster| broadcaster.register(subscription.clone(), id, listener.connection()))?;
                    // Compound mutations
                    let mut compound_result = None;
                    for mutation in mutations {
                        compound_result = subscriptions[event].compound(mutation);
                    }
                    // Report to the parents
                    if let Some(mutation) = compound_result {
                        self.subscribers.iter().try_for_each(|x| x.mutate(mutation.clone()))?;
                    }
                } else {
                    trace!("[Notifier {}] {command} notifying to {id} about {scope:?} is ignored (no mutation)", self.name);
                    // In case we have a sync channel, report that the command was processed.
                    // This is for test only.
                    if let Some(ref sync) = self._sync {
                        let _ = sync.try_send(());
                    }
                }
            } else {
                trace!("[Notifier {}] {command} notifying to {id} about {scope:?} error: listener id not found", self.name);
            }
        } else {
            trace!("[Notifier {}] {command} notifying to {id} about {scope:?} error: event type {event:?} is disabled", self.name);
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

    fn renew_subscriptions(&self) -> Result<()> {
        let subscriptions = self.subscriptions.lock();
        EVENT_TYPE_ARRAY.iter().copied().filter(|x| self.enabled_events[*x] && subscriptions[*x].active()).try_for_each(|x| {
            let mutation = Mutation::new(Command::Start, subscriptions[x].scope());
            self.subscribers.iter().try_for_each(|subscriber| subscriber.mutate(mutation.clone()))?;
            Ok(())
        })
    }

    async fn join(self: Arc<Self>) -> Result<()> {
        trace!("[Notifier {}] joining", self.name);
        if self.started.load(Ordering::SeqCst) {
            debug!("[Notifier {}] stopping collectors", self.name);
            join_all(self.collectors.iter().map(|x| x.clone().join()))
                .await
                .into_iter()
                .collect::<std::result::Result<Vec<()>, _>>()?;
            debug!("[Notifier {}] stopped collectors", self.name);

            // Once collectors exit, we can signal broadcasters
            self.notification_channel.sender.close();

            debug!("[Notifier {}] stopping broadcasters", self.name);
            join_all(self.broadcasters.iter().map(|x| x.join())).await.into_iter().collect::<std::result::Result<Vec<()>, _>>()?;

            // Once broadcasters exit, we can close the subscribers
            self.subscribers.iter().for_each(|s| s.close());

            debug!("[Notifier {}] stopping subscribers", self.name);
            join_all(self.subscribers.iter().map(|x| x.join())).await.into_iter().collect::<std::result::Result<Vec<()>, _>>()?;

            // Finally, we close all listeners, propagating shutdown by closing their channel when they have one
            // Note that unregistering listeners is no longer meaningful since both broadcasters and subscribers were stopped
            debug!("[Notifier {}] closing listeners", self.name);
            self.listeners.lock().values().for_each(|x| x.close());
        } else {
            trace!("[Notifier {}] join ignored since it was never started", self.name);
            return Err(Error::AlreadyStoppedError);
        }
        debug!("[Notifier {}] terminated", self.name);
        Ok(())
    }
}

// #[cfg(test)]
pub mod test_helpers {
    use super::*;
    use crate::{
        address::test_helpers::get_3_addresses,
        connection::ChannelConnection,
        notification::test_helpers::{
            BlockAddedNotification, Data, TestNotification, UtxosChangedNotification, VirtualChainChangedNotification,
        },
        scope::{BlockAddedScope, UtxosChangedScope, VirtualChainChangedScope},
        subscriber::test_helpers::SubscriptionMessage,
    };
    use async_channel::Sender;

    pub type TestConnection = ChannelConnection<TestNotification>;
    pub type TestNotifier = Notifier<TestNotification, ChannelConnection<TestNotification>>;

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

    #[derive(Debug)]
    pub struct Step {
        pub name: &'static str,
        pub mutations: Vec<Option<Mutation>>,
        pub expected_subscriptions: Vec<Option<SubscriptionMessage>>,
        pub notification: TestNotification,
        pub expected_notifications: Vec<Option<TestNotification>>,
    }

    impl Step {
        pub fn set_data(&mut self, data: u64) {
            *self.notification.data_mut() = data;
            self.expected_notifications.iter_mut().for_each(|x| {
                if let Some(notification) = x.as_mut() {
                    *notification.data_mut() = data;
                }
            });
        }
    }

    pub fn overall_test_steps(listener_id: ListenerId) -> Vec<Step> {
        fn m(command: Command) -> Option<Mutation> {
            Some(Mutation { command, scope: Scope::BlockAdded(BlockAddedScope {}) })
        }
        let s = |command: Command| -> Option<SubscriptionMessage> {
            Some(SubscriptionMessage { listener_id, mutation: Mutation { command, scope: Scope::BlockAdded(BlockAddedScope {}) } })
        };
        fn n() -> TestNotification {
            TestNotification::BlockAdded(BlockAddedNotification::default())
        }
        fn e() -> Option<TestNotification> {
            Some(TestNotification::BlockAdded(BlockAddedNotification::default()))
        }

        set_steps_data(vec![
            Step {
                name: "do nothing",
                mutations: vec![],
                expected_subscriptions: vec![],
                notification: n(),
                expected_notifications: vec![None, None],
            },
            Step {
                name: "L0 on",
                mutations: vec![m(Command::Start), None],
                expected_subscriptions: vec![s(Command::Start), None],
                notification: n(),
                expected_notifications: vec![e(), None],
            },
            Step {
                name: "L0 & L1 on",
                mutations: vec![None, m(Command::Start)],
                expected_subscriptions: vec![None, None],
                notification: n(),
                expected_notifications: vec![e(), e()],
            },
            Step {
                name: "L1 on",
                mutations: vec![m(Command::Stop), None],
                expected_subscriptions: vec![None, None],
                notification: n(),
                expected_notifications: vec![None, e()],
            },
            Step {
                name: "all off",
                mutations: vec![None, m(Command::Stop)],
                expected_subscriptions: vec![None, s(Command::Stop)],
                notification: n(),
                expected_notifications: vec![None, None],
            },
        ])
    }

    pub fn virtual_chain_changed_test_steps(listener_id: ListenerId) -> Vec<Step> {
        fn m(command: Command, include_accepted_transaction_ids: bool) -> Option<Mutation> {
            Some(Mutation {
                command,
                scope: Scope::VirtualChainChanged(VirtualChainChangedScope::new(include_accepted_transaction_ids)),
            })
        }
        let s = |command: Command, include_accepted_transaction_ids: bool| -> Option<SubscriptionMessage> {
            Some(SubscriptionMessage {
                listener_id,
                mutation: Mutation {
                    command,
                    scope: Scope::VirtualChainChanged(VirtualChainChangedScope::new(include_accepted_transaction_ids)),
                },
            })
        };
        fn n(accepted_transaction_ids: Option<u64>) -> TestNotification {
            TestNotification::VirtualChainChanged(VirtualChainChangedNotification { data: 0, accepted_transaction_ids })
        }
        fn e(accepted_transaction_ids: Option<u64>) -> Option<TestNotification> {
            Some(TestNotification::VirtualChainChanged(VirtualChainChangedNotification { data: 0, accepted_transaction_ids }))
        }

        set_steps_data(vec![
            Step {
                name: "do nothing",
                mutations: vec![],
                expected_subscriptions: vec![],
                notification: n(None),
                expected_notifications: vec![None, None],
            },
            Step {
                name: "L0+ on",
                mutations: vec![m(Command::Start, true), None],
                expected_subscriptions: vec![s(Command::Start, true), None],
                notification: n(Some(21)),
                expected_notifications: vec![e(Some(21)), None],
            },
            Step {
                name: "L0+ & L1- on",
                mutations: vec![None, m(Command::Start, false)],
                expected_subscriptions: vec![None, None],
                notification: n(Some(42)),
                expected_notifications: vec![e(Some(42)), e(None)],
            },
            Step {
                name: "L0- & L1+ on",
                mutations: vec![m(Command::Start, false), m(Command::Start, true)],
                expected_subscriptions: vec![s(Command::Start, false), s(Command::Start, true)],
                notification: n(Some(63)),
                expected_notifications: vec![e(None), e(Some(63))],
            },
            Step {
                name: "L1+ on",
                mutations: vec![m(Command::Stop, false), None],
                expected_subscriptions: vec![None, None],
                notification: n(Some(84)),
                expected_notifications: vec![None, e(Some(84))],
            },
            Step {
                name: "all off",
                mutations: vec![None, m(Command::Stop, true)],
                expected_subscriptions: vec![None, s(Command::Stop, true)],
                notification: n(Some(21)),
                expected_notifications: vec![None, None],
            },
        ])
    }

    pub fn utxos_changed_test_steps(listener_id: ListenerId) -> Vec<Step> {
        let a_stock = get_3_addresses(true);

        let a = |indexes: &[usize]| indexes.iter().map(|idx| (a_stock[*idx]).clone()).collect::<Vec<_>>();
        let m = |command: Command, indexes: &[usize]| {
            Some(Mutation { command, scope: Scope::UtxosChanged(UtxosChangedScope::new(a(indexes))) })
        };
        let s = |command: Command, indexes: &[usize]| {
            Some(SubscriptionMessage {
                listener_id,
                mutation: Mutation { command, scope: Scope::UtxosChanged(UtxosChangedScope::new(a(indexes))) },
            })
        };
        let n =
            |indexes: &[usize]| TestNotification::UtxosChanged(UtxosChangedNotification { data: 0, addresses: Arc::new(a(indexes)) });
        let e = |indexes: &[usize]| {
            Some(TestNotification::UtxosChanged(UtxosChangedNotification { data: 0, addresses: Arc::new(a(indexes)) }))
        };

        set_steps_data(vec![
            Step {
                name: "do nothing",
                mutations: vec![],
                expected_subscriptions: vec![],
                notification: n(&[]),
                expected_notifications: vec![None, None, None],
            },
            Step {
                name: "L0[0] <= N[0]",
                mutations: vec![m(Command::Start, &[0]), None, None],
                expected_subscriptions: vec![s(Command::Start, &[0]), None, None],
                notification: n(&[0]),
                expected_notifications: vec![e(&[0]), None, None],
            },
            Step {
                name: "L0[0] <= N[0,1,2]",
                mutations: vec![m(Command::Start, &[0]), None, None],
                expected_subscriptions: vec![None, None, None],
                notification: n(&[0, 1, 2]),
                expected_notifications: vec![e(&[0]), None, None],
            },
            Step {
                name: "L0[0], L1[1] <= N[0,1,2]",
                mutations: vec![None, m(Command::Start, &[1]), None],
                expected_subscriptions: vec![None, s(Command::Start, &[1]), None],
                notification: n(&[0, 1, 2]),
                expected_notifications: vec![e(&[0]), e(&[1]), None],
            },
            Step {
                name: "L0[0], L1[1], L2[2] <= N[0,1,2]",
                mutations: vec![None, None, m(Command::Start, &[2])],
                expected_subscriptions: vec![None, None, s(Command::Start, &[2])],
                notification: n(&[0, 1, 2]),
                expected_notifications: vec![e(&[0]), e(&[1]), e(&[2])],
            },
            Step {
                name: "L0[0, 2], L1[*], L2[1, 2] <= N[0,1,2]",
                mutations: vec![m(Command::Start, &[2]), m(Command::Start, &[]), m(Command::Start, &[1])],
                expected_subscriptions: vec![None, s(Command::Start, &[]), None],
                notification: n(&[0, 1, 2]),
                expected_notifications: vec![e(&[0, 2]), e(&[0, 1, 2]), e(&[1, 2])],
            },
            Step {
                name: "L0[0, 2], L1[*], L2[1, 2] <= N[0]",
                mutations: vec![None, None, None],
                expected_subscriptions: vec![None, None, None],
                notification: n(&[0]),
                expected_notifications: vec![e(&[0]), e(&[0]), None],
            },
            Step {
                name: "L0[2], L1[1], L2[*] <= N[0, 1]",
                mutations: vec![m(Command::Stop, &[0]), m(Command::Start, &[1]), m(Command::Start, &[])],
                expected_subscriptions: vec![None, s(Command::Start, &[1, 2]), s(Command::Start, &[])],
                notification: n(&[0, 1]),
                expected_notifications: vec![None, e(&[1]), e(&[0, 1])],
            },
            Step {
                name: "L2[*] <= N[0, 1, 2]",
                mutations: vec![m(Command::Stop, &[]), m(Command::Stop, &[1]), m(Command::Stop, &[1])],
                expected_subscriptions: vec![None, None, None],
                notification: n(&[0, 1, 2]),
                expected_notifications: vec![None, None, e(&[0, 1, 2])],
            },
            Step {
                name: "all off",
                mutations: vec![None, None, m(Command::Stop, &[])],
                expected_subscriptions: vec![None, None, s(Command::Stop, &[])],
                notification: n(&[0, 1, 2]),
                expected_notifications: vec![None, None, None],
            },
        ])
    }

    fn set_steps_data(mut steps: Vec<Step>) -> Vec<Step> {
        // Prepare the notification data markers for the test
        for (idx, step) in steps.iter_mut().enumerate() {
            step.set_data(idx as u64);
        }
        steps
    }
}

#[cfg(test)]
mod tests {
    use super::{test_helpers::*, *};
    use crate::{
        collector::CollectorFrom,
        connection::ChannelType,
        converter::ConverterFrom,
        events::EVENT_TYPE_ARRAY,
        notification::test_helpers::*,
        subscriber::test_helpers::{SubscriptionManagerMock, SubscriptionMessage},
    };
    use async_channel::{unbounded, Receiver, Sender};

    const SUBSCRIPTION_MANAGER_ID: u64 = 0;

    struct Test {
        name: &'static str,
        notifier: Arc<TestNotifier>,
        subscription_receiver: Receiver<SubscriptionMessage>,
        listeners: Vec<ListenerId>,
        notification_sender: Sender<TestNotification>,
        notification_receivers: Vec<Receiver<TestNotification>>,
        sync_receiver: Receiver<()>,
        steps: Vec<Step>,
    }

    impl Test {
        fn new(name: &'static str, listener_count: usize, steps: Vec<Step>) -> Self {
            type TestConverter = ConverterFrom<TestNotification, TestNotification>;
            type TestCollector = CollectorFrom<TestConverter>;
            // Build the full-featured notifier
            let (sync_sender, sync_receiver) = unbounded();
            let (notification_sender, notification_receiver) = unbounded();
            let (subscription_sender, subscription_receiver) = unbounded();
            let collector = Arc::new(TestCollector::new("test", notification_receiver, Arc::new(TestConverter::new())));
            let subscription_manager = Arc::new(SubscriptionManagerMock::new(subscription_sender));
            let subscriber =
                Arc::new(Subscriber::new("test", EVENT_TYPE_ARRAY[..].into(), subscription_manager, SUBSCRIPTION_MANAGER_ID));
            let notifier = Arc::new(TestNotifier::with_sync(
                "test",
                EVENT_TYPE_ARRAY[..].into(),
                vec![collector],
                vec![subscriber],
                1,
                Some(sync_sender),
            ));
            // Create the listeners
            let mut listeners = Vec::with_capacity(listener_count);
            let mut notification_receivers = Vec::with_capacity(listener_count);
            for _ in 0..listener_count {
                let (sender, receiver) = unbounded();
                let connection = TestConnection::new(sender, ChannelType::Closable);
                listeners.push(notifier.register_new_listener(connection));
                notification_receivers.push(receiver);
            }
            // Return the built test object
            Self {
                name,
                notifier,
                subscription_receiver,
                listeners,
                notification_sender,
                notification_receivers,
                sync_receiver,
                steps,
            }
        }

        async fn run(&self) {
            self.notifier.clone().start();

            // Execute the test steps
            for (step_idx, step) in self.steps.iter().enumerate() {
                trace!("Execute test step #{step_idx}: {}", step.name);
                // Apply the subscription mutations and check the yielded subscriptions messages
                // the subscription manager gets
                for (idx, mutation) in step.mutations.iter().enumerate() {
                    if let Some(ref mutation) = mutation {
                        trace!("Mutation #{idx}");
                        assert!(
                            self.notifier
                                .execute_subscribe_command(self.listeners[idx], mutation.scope.clone(), mutation.command)
                                .await
                                .is_ok(),
                            "executing the subscription command {mutation:?} failed"
                        );
                        trace!("Receiving sync message #{step_idx} after subscribing");
                        assert!(
                            self.sync_receiver.recv().await.is_ok(),
                            "{} - {}: receiving a sync message failed",
                            self.name,
                            step.name
                        );
                        if let Some(ref expected_subscription) = step.expected_subscriptions[idx] {
                            let subscription = self.subscription_receiver.recv().await.unwrap();
                            assert_eq!(
                                *expected_subscription, subscription,
                                "{} - {}: the listener[{}] mutation {mutation:?} yielded the wrong subscription",
                                self.name, step.name, idx
                            );
                        } else {
                            assert!(
                                self.subscription_receiver.is_empty(),
                                "{} - {}: listener[{}] mutation {mutation:?} yielded a subscription but should not",
                                self.name,
                                step.name,
                                idx
                            );
                        }
                    }
                }

                // Send the notification
                trace!("Sending notification #{step_idx}");
                assert!(
                    self.notification_sender.send_blocking(step.notification.clone()).is_ok(),
                    "{} - {}: sending the notification failed",
                    self.name,
                    step.name
                );
                trace!("Receiving sync message #{step_idx} after notifying");
                assert!(self.sync_receiver.recv().await.is_ok(), "{} - {}: receiving a sync message failed", self.name, step.name);

                // Check what the listeners do receive
                for (idx, expected_notifications) in step.expected_notifications.iter().enumerate() {
                    if let Some(ref expected_notifications) = expected_notifications {
                        let notification = self.notification_receivers[idx].recv().await.unwrap();
                        assert_eq!(
                            *expected_notifications, notification,
                            "{} - {}: listener[{}] got wrong notification",
                            self.name, step.name, idx
                        );
                    } else {
                        assert!(
                            self.notification_receivers[idx].is_empty(),
                            "{} - {}: listener[{}] has a notification in its channel but should not",
                            self.name,
                            step.name,
                            idx
                        );
                    }
                }
            }
            self.notification_sender.close();
            assert!(self.notifier.join().await.is_ok(), "notifier failed to stop");
        }
    }

    #[tokio::test]
    async fn test_overall() {
        kaspa_core::log::try_init_logger("trace,kaspa_notify=trace");
        let test = Test::new("BlockAdded broadcast (OverallSubscription type)", 2, overall_test_steps(SUBSCRIPTION_MANAGER_ID));
        test.run().await;
    }

    #[tokio::test]
    async fn test_virtual_chain_changed() {
        kaspa_core::log::try_init_logger("trace,kaspa_notify=trace");
        let test = Test::new("VirtualChainChanged broadcast", 2, virtual_chain_changed_test_steps(SUBSCRIPTION_MANAGER_ID));
        test.run().await;
    }

    #[tokio::test]
    async fn test_utxos_changed() {
        kaspa_core::log::try_init_logger("trace,kaspa_notify=trace");
        let test = Test::new("UtxosChanged broadcast", 3, utxos_changed_test_steps(SUBSCRIPTION_MANAGER_ID));
        test.run().await;
    }
}
