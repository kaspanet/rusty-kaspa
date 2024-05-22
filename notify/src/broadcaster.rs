extern crate derive_more;
use crate::{
    connection::Connection,
    error::Result,
    events::{EventArray, EventType},
    listener::ListenerId,
    notification::Notification,
    subscription::{context::SubscriptionContext, BroadcastingSingle, DynSubscription},
};
use async_channel::{Receiver, Sender};
use core::fmt::Debug;
use derive_more::Deref;
use futures::{future::FutureExt, select_biased};
use indexmap::IndexMap;
use kaspa_core::{debug, trace};
use std::{
    collections::HashMap,
    fmt::Display,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use workflow_core::channel::Channel;

type ConnectionSet<T> = HashMap<ListenerId, T>;

/// Broadcasting plan structured by subscription, encoding and connection
#[derive(Deref)]
struct Plan<C: Connection>(IndexMap<DynSubscription, HashMap<C::Encoding, ConnectionSet<C>>>);

impl<C> Plan<C>
where
    C: Connection,
{
    fn insert(&mut self, subscription: DynSubscription, id: ListenerId, connection: C) -> Option<C> {
        // Make sure only one instance of ìd` is registered in the whole object
        let result = self.remove(&id);
        let encoding = connection.encoding();
        self.0.entry(subscription.clone()).or_default().entry(encoding).or_default().entry(id).or_insert_with(|| {
            #[cfg(test)]
            trace!("Broadcasting plan: insert listener {} with {:?}", id, subscription);
            connection
        });
        result
    }

    fn remove(&mut self, id: &ListenerId) -> Option<C> {
        let mut result = None;
        let mut found_subscription: Option<DynSubscription> = None;
        let mut found_encoding: Option<C::Encoding> = None;
        'outer: for (subscription, encoding_set) in self.0.iter_mut() {
            for (encoding, connection_set) in encoding_set.iter_mut() {
                if let Some(connection) = connection_set.remove(id) {
                    #[cfg(test)]
                    trace!("Broadcasting plan: removed listener {}", id);
                    result = Some(connection);
                    if connection_set.is_empty() {
                        found_encoding = Some(encoding.clone());
                        found_subscription = Some((*subscription).clone());
                        //  The plan is guaranteed to contain no duplicate occurrence of every id
                        break 'outer;
                    }
                }
            }
        }
        // Cleaning empty entries
        if let Some(ref subscription) = found_subscription {
            if let Some(ref encoding) = found_encoding {
                self.0.get_mut(subscription).unwrap().remove(encoding);
                if self.0.get(subscription).unwrap().is_empty() {
                    self.0.swap_remove(subscription);
                }
            }
        }
        result
    }

    // fn len(&self) -> usize {
    //     self.0.values().map(|encodings| encodings.values().map(|connections| connections.len()).count()).count()
    // }
}

impl<C: Connection> Default for Plan<C> {
    fn default() -> Self {
        Self(Default::default())
    }
}

#[derive(Clone, Debug)]
enum Ctl<C>
where
    C: Connection,
{
    Register(DynSubscription, ListenerId, C),
    Unregister(EventType, ListenerId),
}

#[derive(Debug)]
pub(crate) struct Broadcaster<N, C>
where
    N: Notification,
    C: Connection,
{
    name: &'static str,
    index: usize,
    context: SubscriptionContext,
    started: Arc<AtomicBool>,
    ctl: Channel<Ctl<C>>,
    incoming: Receiver<N>,
    shutdown: Channel<()>,
    /// Sync channel, for handling of messages in predictable sequence; exclusively intended for tests.
    _sync: Option<Sender<()>>,
}

impl<N, C> Broadcaster<N, C>
where
    N: Notification,
    C: Connection<Notification = N>,
{
    pub fn new(
        name: &'static str,
        index: usize,
        context: SubscriptionContext,
        incoming: Receiver<N>,
        _sync: Option<Sender<()>>,
    ) -> Self {
        Self {
            name,
            index,
            context,
            started: Arc::new(AtomicBool::default()),
            ctl: Channel::unbounded(),
            incoming,
            _sync,
            shutdown: Channel::oneshot(),
        }
    }

    pub fn start(self: &Arc<Self>) {
        self.clone().spawn_notification_broadcasting_task();
    }

    fn spawn_notification_broadcasting_task(self: Arc<Self>) {
        // The task can only be spawned once
        if self.started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        trace!("[{}] Starting notification broadcasting task", self);
        let context = self.context.clone();
        workflow_core::task::spawn(async move {
            // Broadcasting plan by event type
            let mut plan = EventArray::<Plan<C>>::default();
            // Create a store for closed connections to be removed from the plan
            let mut purge: Vec<ListenerId> = Vec::new();
            loop {
                select_biased! {
                    ctl = self.ctl.recv().fuse() => {
                        if let Ok(ctl) = ctl {
                            match ctl {
                                Ctl::Register(subscription, id, connection) => {
                                    let event_type = subscription.event_type();
                                    plan[event_type].insert(subscription.broadcasting(&context), id, connection);
                                    debug!("[{}] insert {} subscription, count = {}, capacity = {}", self, event_type, plan[event_type].len(), plan[event_type].capacity());
                                },
                                Ctl::Unregister(event_type, id) => {
                                    plan[event_type].remove(&id);
                                    debug!("[{}] remove {} subscription, count = {}, capacity = {}", self, event_type, plan[event_type].len(), plan[event_type].capacity());
                                },
                            }
                        } else {
                            break;
                        }
                    },

                    notification = self.incoming.recv().fuse() => {
                        if let Ok(notification) = notification {
                            // Broadcast the notification...
                            let event = notification.event_type();
                            for (subscription, encoding_set) in plan[event].iter() {
                                // ... by subscription scope
                                if let Some(applied_notification) = notification.apply_subscription(&**subscription, &context) {
                                    for (encoding, connection_set) in encoding_set.iter() {
                                        // ... by message encoding
                                        let message = C::into_message(&applied_notification, encoding);
                                        for (id, connection) in connection_set.iter() {
                                            // ... to listeners connections
                                            match connection.send(message.clone()).await {
                                                Ok(_) => {
                                                    trace!("[{}] sent notification {notification} to listener {id}", self);
                                                },
                                                Err(_) => {
                                                    if connection.is_closed() {
                                                        trace!("[{}] could not send a notification to listener {id} because its connection is closed - removing it", self);
                                                        purge.push(*id);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Remove closed connections
                            purge.drain(..).for_each(|id| { plan[event].remove(&id); });

                        } else {
                            break;
                        }
                    }
                }

                // In case we have a sync channel, report that the command was processed.
                // This is for test only.
                if let Some(ref sync) = self._sync {
                    let _ = sync.try_send(());
                }
            }
            debug!("[{}] notification stream ended", self);
            let _ = self.shutdown.drain();
            let _ = self.shutdown.try_send(());
        });
    }

    pub fn register(&self, subscription: DynSubscription, id: ListenerId, connection: C) -> Result<()> {
        assert!(subscription.active());
        self.ctl.try_send(Ctl::Register(subscription, id, connection))?;
        Ok(())
    }

    pub fn unregister(&self, event_type: EventType, id: ListenerId) -> Result<()> {
        self.ctl.try_send(Ctl::Unregister(event_type, id))?;
        Ok(())
    }

    async fn join_notification_broadcasting_task(&self) -> Result<()> {
        trace!("[{}] joining", self);
        self.shutdown.recv().await?;
        debug!("[{}] terminated", self);
        Ok(())
    }

    pub async fn join(&self) -> Result<()> {
        self.join_notification_broadcasting_task().await
    }
}

impl<N, C> Display for Broadcaster<N, C>
where
    N: Notification,
    C: Connection<Notification = N>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Broadcaster-{}-{}", self.name, self.index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        connection::{ChannelConnection, ChannelType},
        listener::Listener,
        notification::test_helpers::*,
        notifier::test_helpers::{
            overall_test_steps, utxos_changed_test_steps, virtual_chain_changed_test_steps, Step, TestConnection, SYNC_MAX_DELAY,
        },
        subscription::context::SubscriptionContext,
    };
    use async_channel::{unbounded, Sender};
    use tokio::time::timeout;

    type TestBroadcaster = Broadcaster<TestNotification, ChannelConnection<TestNotification>>;

    struct Test {
        name: &'static str,
        broadcaster: Arc<TestBroadcaster>,
        /// Listeners, vector index = ListenerId
        listeners: Vec<Listener<TestConnection>>,
        subscription_context: SubscriptionContext,
        ctl_sender: Sender<Ctl<TestConnection>>,
        sync_receiver: Receiver<()>,
        notification_sender: Sender<TestNotification>,
        notification_receivers: Vec<Receiver<TestNotification>>,
        steps: Vec<Step>,
    }

    impl Test {
        fn new(name: &'static str, listener_count: usize, steps: Vec<Step>) -> Self {
            const IDENT: &str = "test";
            let subscription_context = SubscriptionContext::new();
            let (sync_sender, sync_receiver) = unbounded();
            let (notification_sender, notification_receiver) = unbounded();
            let broadcaster =
                Arc::new(TestBroadcaster::new(IDENT, 0, subscription_context.clone(), notification_receiver, Some(sync_sender)));
            let mut listeners = Vec::with_capacity(listener_count);
            let mut notification_receivers = Vec::with_capacity(listener_count);
            for i in 0..listener_count {
                let (sender, receiver) = unbounded();
                let connection = TestConnection::new(IDENT, sender, ChannelType::Closable);
                let listener = Listener::new(i as ListenerId, connection);
                listeners.push(listener);
                notification_receivers.push(receiver);
            }
            Self {
                name,
                broadcaster: broadcaster.clone(),
                listeners,
                subscription_context,
                ctl_sender: broadcaster.ctl.sender.clone(),
                sync_receiver,
                notification_sender,
                notification_receivers,
                steps,
            }
        }

        async fn run(&mut self) {
            self.broadcaster.start();

            // Execute the test steps
            for (step_idx, step) in self.steps.iter().enumerate() {
                // Apply the subscription mutations and register the changes into the broadcaster
                trace!("{} #{} - Initial Subscription Context {}", self.name, step_idx, self.subscription_context.address_tracker);
                for (idx, mutation) in step.mutations.iter().enumerate() {
                    if let Some(ref mutation) = mutation {
                        trace!("{} #{} - {}: L{} {:?}", self.name, step_idx, step.name, idx, mutation);
                        let event = mutation.event_type();
                        let outcome =
                            self.listeners[idx].mutate(mutation.clone(), Default::default(), &self.subscription_context).unwrap();
                        if outcome.has_new_state() {
                            trace!(
                                "{} #{} - {}: - L{} has the new state {:?}",
                                self.name,
                                step_idx,
                                step.name,
                                idx,
                                self.listeners[idx].subscriptions[event]
                            );
                            let ctl = match mutation.active() {
                                true => Ctl::Register(
                                    self.listeners[idx].subscriptions[event].clone(),
                                    idx as u64,
                                    self.listeners[idx].connection(),
                                ),
                                false => Ctl::Unregister(event, idx as u64),
                            };
                            assert!(
                                self.ctl_sender.send(ctl).await.is_ok(),
                                "{} #{} - {}: sending a registration message failed",
                                self.name,
                                step_idx,
                                step.name
                            );
                            assert!(
                                timeout(SYNC_MAX_DELAY, self.sync_receiver.recv()).await.unwrap().is_ok(),
                                "{} #{} - {}: receiving a sync message failed",
                                self.name,
                                step_idx,
                                step.name
                            );
                        } else if outcome.has_changes() {
                            trace!(
                                "{} #{} - {}: - L{} is inner changed into {:?}",
                                self.name,
                                step_idx,
                                step.name,
                                idx,
                                self.listeners[idx].subscriptions[event]
                            );
                        } else {
                            trace!(
                                "{} #{} - {}: - L{} is unchanged {:?}",
                                self.name,
                                step_idx,
                                step.name,
                                idx,
                                self.listeners[idx].subscriptions[event]
                            );
                        }
                    }
                }

                // Send the notification
                if step_idx == 8 {
                    trace!("#8");
                }
                trace!("{} #{} - {}: sending a notification...", self.name, step_idx, step.name);
                assert!(
                    self.notification_sender.send_blocking(step.notification.clone()).is_ok(),
                    "{} #{} - {}: sending the notification failed",
                    self.name,
                    step_idx,
                    step.name
                );
                trace!("{} #{} - {}: receiving sync signal...", self.name, step_idx, step.name);
                assert!(
                    timeout(SYNC_MAX_DELAY, self.sync_receiver.recv()).await.unwrap().is_ok(),
                    "{} #{} - {}: receiving a sync message failed",
                    self.name,
                    step_idx,
                    step.name
                );

                // Check what the listeners do receive
                for (idx, expected) in step.expected_notifications.iter().enumerate() {
                    if let Some(ref expected) = expected {
                        assert!(
                            !self.notification_receivers[idx].is_empty(),
                            "{} #{} - {}: listener[{}] has no notification in its channel though some is expected",
                            self.name,
                            step_idx,
                            step.name,
                            idx
                        );
                        let notification = self.notification_receivers[idx].recv().await.unwrap();
                        assert_eq!(
                            *expected, notification,
                            "{} #{} - {}: listener[{}] got wrong notification",
                            self.name, step_idx, step.name, idx
                        );
                    } else {
                        assert!(
                            self.notification_receivers[idx].is_empty(),
                            "{} #{} - {}: listener[{}] has a notification in its channel but should not",
                            self.name,
                            step_idx,
                            step.name,
                            idx
                        );
                    }
                }
            }
            self.notification_sender.close();
            assert!(self.broadcaster.join().await.is_ok(), "broadcaster failed to stop");
        }
    }

    #[tokio::test]
    async fn test_overall() {
        kaspa_core::log::try_init_logger("trace,kaspa_notify=trace");
        let mut test = Test::new("BlockAdded broadcast (OverallSubscription type)", 2, overall_test_steps(0));
        test.run().await;
    }

    #[tokio::test]
    async fn test_virtual_chain_changed() {
        kaspa_core::log::try_init_logger("trace,kaspa_notify=trace");
        let mut test = Test::new("VirtualChainChanged broadcast", 2, virtual_chain_changed_test_steps(0));
        test.run().await;
    }

    #[tokio::test]
    async fn test_utxos_changed() {
        kaspa_core::log::try_init_logger("trace,kaspa_notify=trace");
        let mut test = Test::new("UtxosChanged broadcast", 3, utxos_changed_test_steps(0));
        test.run().await;
    }
}
