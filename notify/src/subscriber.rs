use async_trait::async_trait;
use core::fmt::Debug;
use kaspa_core::{debug, trace};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
extern crate derive_more;
use crate::events::{EventSwitches, EventType};

use super::{
    error::Result,
    listener::ListenerId,
    scope::Scope,
    subscription::{Command, Mutation},
};
use workflow_core::channel::Channel;

/// A manager of subscriptions (see [`Scope`]) for registered listeners
#[async_trait]
pub trait SubscriptionManager: Send + Sync + Debug {
    async fn start_notify(&self, id: ListenerId, scope: Scope) -> Result<()>;
    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> Result<()>;

    async fn execute_subscribe_command(&self, id: ListenerId, scope: Scope, command: Command) -> Result<()> {
        match command {
            Command::Start => self.start_notify(id, scope).await,
            Command::Stop => self.stop_notify(id, scope).await,
        }
    }
}

pub type DynSubscriptionManager = Arc<dyn SubscriptionManager>;

/// A subscriber handling subscription messages as [`Mutation`] and executing them into a [SubscriptionManager]
///
/// A subscriber has a set of enabled event type (see [`EventType`]). It only handles subscriptions
/// whose event type is enabled and drops all others.
///
/// A subscriber has a listener ID identifying its owner (usually a [`Notifier`](crate::notifier::Notifier)) as a listener of its manager
/// (usually also a [`Notifier`](crate::notifier::Notifier)).
#[derive(Debug)]
pub struct Subscriber {
    name: &'static str,

    /// Event types this subscriber is configured to subscribe to
    enabled_events: EventSwitches,

    /// Subscription manager
    subscription_manager: DynSubscriptionManager,

    /// Listener ID
    listener_id: ListenerId,

    /// Has this subscriber been started?
    started: Arc<AtomicBool>,

    incoming: Channel<Mutation>,
    shutdown: Channel<()>,
}

impl Subscriber {
    pub fn new(
        name: &'static str,
        enabled_events: EventSwitches,
        subscription_manager: DynSubscriptionManager,
        listener_id: ListenerId,
    ) -> Self {
        Self {
            name,
            enabled_events,
            subscription_manager,
            listener_id,
            started: Arc::new(AtomicBool::default()),
            incoming: Channel::unbounded(),
            shutdown: Channel::oneshot(),
        }
    }

    pub fn handles_event_type(&self, event_type: EventType) -> bool {
        self.enabled_events[event_type]
    }

    pub fn start(self: &Arc<Self>) {
        self.clone().spawn_subscription_receiver_task();
    }

    /// Launch the subscription receiver
    fn spawn_subscription_receiver_task(self: Arc<Self>) {
        // The task can only be spawned once
        if self.started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        trace!("[Subscriber {}] starting subscription receiving task", self.name);
        workflow_core::task::spawn(async move {
            while let Ok(mutation) = self.incoming.recv().await {
                if self.handles_event_type(mutation.event_type()) {
                    if let Err(err) = self
                        .subscription_manager
                        .clone()
                        .execute_subscribe_command(self.listener_id, mutation.scope, mutation.command)
                        .await
                    {
                        trace!("[Subscriber {}] the subscription command returned an error: {:?}", self.name, err);
                    }
                }
            }

            debug!("[Subscriber {}] subscription stream ended", self.name);
            let _ = self.shutdown.drain();
            let _ = self.shutdown.try_send(());
        });
    }

    pub fn mutate(self: &Arc<Self>, mutation: Mutation) -> Result<()> {
        self.incoming.try_send(mutation)?;
        Ok(())
    }

    async fn join_subscription_receiver_task(self: &Arc<Self>) -> Result<()> {
        self.shutdown.recv().await?;
        Ok(())
    }

    pub async fn join(self: &Arc<Self>) -> Result<()> {
        trace!("[Subscriber {}] joining", self.name);
        let result = self.join_subscription_receiver_task().await;
        debug!("[Subscriber {}] terminated", self.name);
        result
    }

    pub fn close(&self) {
        self.incoming.sender.close();
    }
}

pub mod test_helpers {
    use super::*;
    use async_channel::Sender;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct SubscriptionMessage {
        pub listener_id: ListenerId,
        pub mutation: Mutation,
    }

    impl SubscriptionMessage {
        pub fn new(listener_id: ListenerId, command: Command, scope: Scope) -> Self {
            Self { listener_id, mutation: Mutation::new(command, scope) }
        }
    }

    #[derive(Debug)]
    pub struct SubscriptionManagerMock {
        sender: Sender<SubscriptionMessage>,
    }

    impl SubscriptionManagerMock {
        pub fn new(sender: Sender<SubscriptionMessage>) -> Self {
            Self { sender }
        }
    }

    #[async_trait]
    impl SubscriptionManager for SubscriptionManagerMock {
        async fn start_notify(&self, id: ListenerId, scope: Scope) -> Result<()> {
            Ok(self.sender.send(SubscriptionMessage::new(id, Command::Start, scope)).await?)
        }

        async fn stop_notify(&self, id: ListenerId, scope: Scope) -> Result<()> {
            Ok(self.sender.send(SubscriptionMessage::new(id, Command::Stop, scope)).await?)
        }
    }
}
