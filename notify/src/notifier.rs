use crate::{
    events::EVENT_TYPE_ARRAY,
    listener::ListenerLifespan,
    subscription::{context::SubscriptionContext, MutationPolicies, UtxosChangedMutationPolicy},
};

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
use itertools::Itertools;
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

/// A notifier is a notification broadcaster. It receives notifications from upstream _parents_ and
/// broadcasts those downstream to its _children_ listeners. Symmetrically, it receives subscriptions
/// from its downward listeners, compounds those internally and pushes upward the subscriptions resulting
/// of the compounding, if any, to the _parents_.
///
/// ### Enabled event types
///
/// A notifier has a set of enabled event type (see [`EventType`]). It only broadcasts notifications whose
/// event type is enabled and drops the others. The same goes for subscriptions.
///
/// Each subscriber has a set of enabled event type. No two subscribers may have the same event type enabled.
/// The union of the sets of all subscribers should match the set of the notifier, though this is not mandatory.
///
/// ### Mutation policies
///
/// The notifier is built with some mutation policies defining how an processed listener mutation must be propagated
/// to the _parent_.
///
/// ### Architecture
///
/// #### Internal structure
///
/// The notifier notably owns:
///
/// - a vector of [`DynCollector`]
/// - a vector of [`Subscriber`]
/// - a pool of [`Broadcaster`]
/// - a map of [`Listener`]
///
/// Collectors and subscribers form the scaffold. They are provided to the ctor, are immutable and share its
/// lifespan. Both do materialize a connection to the notifier _parents_, collectors for incoming notifications
/// and subscribers for outgoing subscriptions. They may usually be paired by index in their respective
/// vector but this by no means is mandatory, opening a field for special edge cases.
///
/// The broadcasters are built in the ctor according to a provided count. They act as a pool of workers competing
/// for the processing of an incoming notification.
///
/// The listeners are managed dynamically through registration/unregistration calls.
///
/// #### External conformation
///
/// The notifier is designed so that many instances can be interconnected and form a DAG of notifiers.
///
/// However, the notifications path from the root all the way downstream to the final clients is forming a tree,
/// not a DAG. This is because, for a given type of notification (see [`EventType`]), a notifier has at most a single
/// _parent_ provider.
///
/// The same is symmetrically true about subscriptions which travel upstream from clients to the root along a tree,
/// meaning that, for a given type of subscription (see [`EventType`]), a notifier has at most a single subscriber,
/// targeting a single _parent_.
///
/// ### Special considerations
///
/// A notifier is built with a specific set of enabled event types. It is however possible to manually subscribe
/// to a disabled scope and thus have a custom-made collector of the notifier receive notifications of this disabled scope,
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
        subscription_context: SubscriptionContext,
        broadcasters: usize,
        policies: MutationPolicies,
    ) -> Self {
        Self::with_sync(name, enabled_events, collectors, subscribers, subscription_context, broadcasters, policies, None)
    }

    pub fn with_sync(
        name: &'static str,
        enabled_events: EventSwitches,
        collectors: Vec<DynCollector<N>>,
        subscribers: Vec<Arc<Subscriber>>,
        subscription_context: SubscriptionContext,
        broadcasters: usize,
        policies: MutationPolicies,
        _sync: Option<Sender<()>>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner::new(
                name,
                enabled_events,
                collectors,
                subscribers,
                subscription_context,
                broadcasters,
                policies,
                _sync,
            )),
        }
    }

    pub fn subscription_context(&self) -> &SubscriptionContext {
        &self.inner.subscription_context
    }

    pub fn enabled_events(&self) -> &EventSwitches {
        &self.inner.enabled_events
    }

    pub fn start(self: Arc<Self>) {
        self.inner.clone().start(self.clone());
    }

    pub fn register_new_listener(&self, connection: C, lifespan: ListenerLifespan) -> ListenerId {
        self.inner.register_new_listener(connection, lifespan)
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

    /// Enabled Subscriber by event type
    enabled_subscriber: EventArray<Option<Arc<Subscriber>>>,

    /// Subscription context
    subscription_context: SubscriptionContext,

    /// Mutation policies
    policies: MutationPolicies,

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
        subscription_context: SubscriptionContext,
        broadcasters: usize,
        policies: MutationPolicies,
        _sync: Option<Sender<()>>,
    ) -> Self {
        assert!(broadcasters > 0, "a notifier requires a minimum of one broadcaster");
        let notification_channel = Channel::unbounded();
        let broadcasters = (0..broadcasters)
            .map(|idx| {
                Arc::new(Broadcaster::new(
                    name,
                    idx,
                    subscription_context.clone(),
                    notification_channel.receiver.clone(),
                    _sync.clone(),
                ))
            })
            .collect::<Vec<_>>();
        let enabled_subscriber = EventArray::from_fn(|index| {
            let event: EventType = index.try_into().unwrap();
            let mut iter = subscribers.iter().filter(|&x| x.handles_event_type(event)).cloned();
            let subscriber = iter.next();
            assert!(iter.next().is_none(), "A notifier is not allowed to have more than one subscriber per event type");
            subscriber
        });
        let utxos_changed_capacity = match policies.utxo_changed {
            UtxosChangedMutationPolicy::AddressSet => subscription_context.address_tracker.addresses_preallocation(),
            UtxosChangedMutationPolicy::Wildcard => None,
        };
        Self {
            enabled_events,
            listeners: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(ArrayBuilder::compounded(utxos_changed_capacity)),
            started: Arc::new(AtomicBool::new(false)),
            notification_channel,
            broadcasters,
            collectors,
            subscribers,
            enabled_subscriber,
            subscription_context,
            policies,
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

    fn register_new_listener(self: &Arc<Self>, connection: C, lifespan: ListenerLifespan) -> ListenerId {
        let mut listeners = self.listeners.lock();
        loop {
            let id = u64::from_le_bytes(rand::random::<[u8; 8]>());

            // This is very unlikely to happen but still, check for duplicates
            if let Entry::Vacant(e) = listeners.entry(id) {
                trace!("[Notifier {}] registering listener {id}", self.name);
                let listener = match lifespan {
                    ListenerLifespan::Static(policies) => Listener::new_static(id, connection, &self.subscription_context, policies),
                    ListenerLifespan::Dynamic => Listener::new(id, connection),
                };
                e.insert(listener);
                return id;
            }
        }
    }

    fn unregister_listener(self: &Arc<Self>, id: ListenerId) -> Result<()> {
        // Try to remove the listener, preventing any possible new subscription
        let listener = self.listeners.lock().remove(&id);
        if let Some(mut listener) = listener {
            trace!("[Notifier {}] unregistering listener {id}", self.name);

            // Cancel all remaining active subscriptions
            let mut events = listener
                .subscriptions
                .iter()
                .filter_map(|subscription| if subscription.active() { Some(subscription.event_type()) } else { None })
                .collect_vec();
            events.drain(..).for_each(|event| {
                let _ = self.execute_subscribe_command_impl(id, &mut listener, event.into(), Command::Stop);
            });

            // Close the listener
            trace!("[Notifier {}] closing listener {id}", self.name);
            listener.close();
        } else {
            trace!("[Notifier {}] unregistering listener {id} error: unknown listener id", self.name);
        }
        Ok(())
    }

    pub fn execute_subscribe_command(&self, id: ListenerId, scope: Scope, command: Command) -> Result<()> {
        let event = scope.event_type();
        if self.enabled_events[event] {
            let mut listeners = self.listeners.lock();
            if let Some(listener) = listeners.get_mut(&id) {
                self.execute_subscribe_command_impl(id, listener, scope, command)?;
            } else {
                trace!("[Notifier {}] {command} notifying listener {id} about {scope} error: listener id not found", self.name);
            }
        } else {
            trace!("[Notifier {}] {command} notifying listener {id} about {scope} error: event type {event:?} is disabled", self.name);
            return Err(Error::EventTypeDisabled);
        }
        Ok(())
    }

    fn execute_subscribe_command_impl(
        &self,
        id: ListenerId,
        listener: &mut Listener<C>,
        scope: Scope,
        command: Command,
    ) -> Result<()> {
        let mut sync_feedback: bool = false;
        let event = scope.event_type();
        let scope_trace = format!("{scope}");
        debug!("[Notifier {}] {command} notifying about {scope_trace} to listener {id} - {}", self.name, listener.connection());
        let outcome = listener.mutate(Mutation::new(command, scope), self.policies, &self.subscription_context)?;
        if outcome.has_changes() {
            trace!(
                "[Notifier {}] {command} notifying listener {id} about {scope_trace} involves {} mutations",
                self.name,
                outcome.mutations.len(),
            );
            // Update broadcasters
            match (listener.subscriptions[event].active(), outcome.mutated) {
                (true, Some(subscription)) => {
                    self.broadcasters
                        .iter()
                        .try_for_each(|broadcaster| broadcaster.register(subscription.clone(), id, listener.connection()))?;
                }
                (true, None) => {
                    sync_feedback = true;
                }
                (false, _) => {
                    self.broadcasters.iter().try_for_each(|broadcaster| broadcaster.unregister(event, id))?;
                }
            }
            self.apply_mutations(event, outcome.mutations, &self.subscription_context)?;
        } else {
            trace!("[Notifier {}] {command} notifying listener {id} about {scope_trace} is ignored (no mutation)", self.name);
            sync_feedback = true;
        }
        if sync_feedback {
            // In case we have a sync channel, report that the command was processed.
            // This is for test only.
            if let Some(ref sync) = self._sync {
                let _ = sync.try_send(());
            }
        }
        Ok(())
    }

    fn apply_mutations(&self, event: EventType, mutations: Vec<Mutation>, context: &SubscriptionContext) -> Result<()> {
        let mut subscriptions = self.subscriptions.lock();
        // Compound mutations
        let mut compound_result = None;
        for mutation in mutations {
            compound_result = subscriptions[event].compound(mutation, context);
        }
        // Report to the parent if any
        if let Some(mutation) = compound_result {
            if let Some(ref subscriber) = self.enabled_subscriber[event] {
                subscriber.mutate(mutation)?;
            }
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
            let mutation = Mutation::new(Command::Start, subscriptions[x].scope(&self.subscription_context));
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
            let listener_ids = self.listeners.lock().keys().cloned().collect_vec();
            listener_ids.iter().for_each(|id| {
                let listener = self.listeners.lock().remove(id);
                if let Some(listener) = listener {
                    listener.close();
                }
            });
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
    use std::time::Duration;

    pub const SYNC_MAX_DELAY: Duration = Duration::from_secs(2);

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
    use tokio::time::timeout;

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
            const IDENT: &str = "test";
            type TestConverter = ConverterFrom<TestNotification, TestNotification>;
            type TestCollector = CollectorFrom<TestConverter>;
            // Build the full-featured notifier
            let (sync_sender, sync_receiver) = unbounded();
            let (notification_sender, notification_receiver) = unbounded();
            let (subscription_sender, subscription_receiver) = unbounded();
            let collector = Arc::new(TestCollector::new(IDENT, notification_receiver, Arc::new(TestConverter::new())));
            let subscription_manager = Arc::new(SubscriptionManagerMock::new(subscription_sender));
            let subscription_context = SubscriptionContext::new();
            let subscriber =
                Arc::new(Subscriber::new("test", EVENT_TYPE_ARRAY[..].into(), subscription_manager, SUBSCRIPTION_MANAGER_ID));
            let notifier = Arc::new(TestNotifier::with_sync(
                "test",
                EVENT_TYPE_ARRAY[..].into(),
                vec![collector],
                vec![subscriber],
                subscription_context,
                1,
                Default::default(),
                Some(sync_sender),
            ));
            // Create the listeners
            let mut listeners = Vec::with_capacity(listener_count);
            let mut notification_receivers = Vec::with_capacity(listener_count);
            for _ in 0..listener_count {
                let (sender, receiver) = unbounded();
                let connection = TestConnection::new(IDENT, sender, ChannelType::Closable);
                listeners.push(notifier.register_new_listener(connection, ListenerLifespan::Dynamic));
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
                            timeout(SYNC_MAX_DELAY, self.sync_receiver.recv()).await.unwrap().is_ok(),
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
                            assert!(
                                self.subscription_receiver.is_empty(),
                                "{} - {}: listener[{}] mutation {mutation:?} yielded an extra subscription but should not",
                                self.name,
                                step.name,
                                idx
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
                assert!(
                    timeout(SYNC_MAX_DELAY, self.sync_receiver.recv()).await.unwrap().is_ok(),
                    "{} - {}: receiving a sync message failed",
                    self.name,
                    step.name
                );

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
