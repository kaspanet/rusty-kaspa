use crate::{
    error::Result,
    events::{EventArray, EventType},
    listener::ListenerId,
    notification::Notification,
    notifier::Notify,
    scope::Scope,
    subscriber::SubscriptionManager,
    subscription::{
        array::ArrayBuilder, context::SubscriptionContext, Command, DynSubscription, MutateSingle, Mutation, MutationPolicies,
        UtxosChangedMutationPolicy,
    },
};
use async_channel::Sender;
use async_trait::async_trait;
use kaspa_core::{debug, trace};
use parking_lot::RwLock;
use std::sync::Arc;

/// Root of a notification system
///
/// The [`Root`] receives new notifications via its `notify` function, transforms them according
/// to its internal subscription scope and, when any, sends them through a channel.
///
/// It is a [`SubscriptionManager`], so the notification scope can be dynamically configured
/// according to the needs of the whole notification system.
#[derive(Clone, Debug)]
pub struct Root<N>
where
    N: Notification,
{
    inner: Arc<Inner<N>>,
}

impl<N> Root<N>
where
    N: Notification,
{
    pub fn new(sender: Sender<N>) -> Self {
        let subscription_context = SubscriptionContext::new();
        Self::with_context(sender, subscription_context)
    }

    pub fn with_context(sender: Sender<N>, subscription_context: SubscriptionContext) -> Self {
        let inner = Arc::new(Inner::new(sender, subscription_context));
        Self { inner }
    }

    pub fn send(&self, notification: N) -> Result<()> {
        self.inner.send(notification)
    }

    pub fn close(&self) -> bool {
        debug!("[Notification root] closing");
        self.inner.sender.close()
    }

    pub fn is_closed(&self) -> bool {
        self.inner.sender.is_closed()
    }

    pub fn has_subscription(&self, event: EventType) -> bool {
        self.inner.has_subscription(event)
    }
}

impl<N> Notify<N> for Root<N>
where
    N: Notification,
{
    fn notify(&self, notification: N) -> Result<()> {
        self.inner.notify(notification)
    }
}

#[async_trait]
impl<N> SubscriptionManager for Root<N>
where
    N: Notification,
{
    async fn start_notify(&self, _: ListenerId, scope: Scope) -> Result<()> {
        trace!("[Notification root] start sending notifications of scope {scope:?}");
        self.inner.start_notify(scope)?;
        Ok(())
    }

    async fn stop_notify(&self, _: ListenerId, scope: Scope) -> Result<()> {
        trace!("[Notification root] stop notifications of scope {scope:?}");
        self.inner.stop_notify(scope)?;
        Ok(())
    }
}

#[derive(Debug)]
struct Inner<N>
where
    N: Notification,
{
    sender: Sender<N>,
    subscriptions: RwLock<EventArray<DynSubscription>>,
    subscription_context: SubscriptionContext,
    policies: MutationPolicies,
}

impl<N> Inner<N>
where
    N: Notification,
{
    const ROOT_LISTENER_ID: ListenerId = 1;

    fn new(sender: Sender<N>, subscription_context: SubscriptionContext) -> Self {
        let subscriptions = RwLock::new(ArrayBuilder::single(Self::ROOT_LISTENER_ID, None));
        let policies = MutationPolicies::new(UtxosChangedMutationPolicy::Wildcard);
        Self { sender, subscriptions, subscription_context, policies }
    }

    fn send(&self, notification: N) -> Result<()> {
        let event = notification.event_type();
        let subscription = &self.subscriptions.read()[event];
        if let Some(applied_notification) = notification.apply_subscription(&**subscription, &self.subscription_context) {
            self.sender.try_send(applied_notification)?;
        }
        Ok(())
    }

    pub fn execute_subscribe_command(&self, scope: Scope, command: Command) -> Result<()> {
        let mutation = Mutation::new(command, scope);
        let mut subscriptions = self.subscriptions.write();
        subscriptions[mutation.event_type()].mutate(mutation, self.policies, &self.subscription_context)?;
        Ok(())
    }

    fn start_notify(&self, scope: Scope) -> Result<()> {
        self.execute_subscribe_command(scope, Command::Start)
    }

    fn notify(&self, notification: N) -> Result<()> {
        let event = notification.event_type();
        let subscription = &self.subscriptions.read()[event];
        if subscription.active() {
            if let Some(applied_notification) = notification.apply_subscription(&**subscription, &self.subscription_context) {
                self.sender.try_send(applied_notification)?;
            }
        }
        Ok(())
    }

    fn stop_notify(&self, scope: Scope) -> Result<()> {
        self.execute_subscribe_command(scope, Command::Stop)
    }

    fn has_subscription(&self, event: EventType) -> bool {
        let subscription = &self.subscriptions.read()[event];
        subscription.active()
    }
}

// TODO: tests
