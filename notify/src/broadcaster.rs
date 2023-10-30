extern crate derive_more;
use super::{
    connection::Connection, error::Result, events::EventArray, listener::ListenerId, notification::Notification,
    subscription::DynSubscription,
};
use async_channel::{Receiver, Sender};
use core::fmt::Debug;
use derive_more::Deref;
use futures::{future::FutureExt, select};
use kaspa_core::{debug, trace};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use workflow_core::channel::Channel;

type ConnectionSet<T> = HashMap<ListenerId, T>;

/// Broadcast plan
#[derive(Deref)]
struct Plan<C: Connection>(HashMap<DynSubscription, HashMap<C::Encoding, ConnectionSet<C>>>);

impl<C> Plan<C>
where
    C: Connection,
{
    fn insert(&mut self, subscription: DynSubscription, id: ListenerId, connection: C) -> Option<C> {
        // Make sure only one instance of ìd` is registered in the whole object
        let result = self.remove(&id);
        let encoding = connection.encoding();
        self.0.entry(subscription).or_default().entry(encoding).or_default().entry(id).or_insert(connection);
        result
    }

    fn remove(&mut self, id: &ListenerId) -> Option<C> {
        let mut result = None;
        let mut found_subscription: Option<DynSubscription> = None;
        let mut found_encoding: Option<C::Encoding> = None;
        'outer: for (subscription, encoding_set) in self.0.iter_mut() {
            for (encoding, connection_set) in encoding_set.iter_mut() {
                if let Some(connection) = connection_set.remove(id) {
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
                    self.0.remove(subscription);
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
    Unregister(DynSubscription, ListenerId),
}

#[derive(Debug)]
pub(crate) struct Broadcaster<N, C>
where
    N: Notification,
    C: Connection,
{
    name: &'static str,
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
    pub fn new(name: &'static str, incoming: Receiver<N>, _sync: Option<Sender<()>>) -> Self {
        Self {
            name,
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
        trace!("[Broadcaster-{}] Starting notification broadcasting task", self.name);
        workflow_core::task::spawn(async move {
            // Broadcasting plan by event type
            let mut plan = EventArray::<Plan<C>>::default();
            // Create a store for closed connections to be removed from the plan
            let mut purge: Vec<ListenerId> = Vec::new();
            loop {
                select! {
                    ctl = self.ctl.recv().fuse() => {
                        if let Ok(ctl) = ctl {
                            match ctl {
                                Ctl::Register(subscription, id, connection) => {
                                    plan[subscription.event_type()].insert(subscription, id, connection);
                                },
                                Ctl::Unregister(subscription, id) => {
                                    plan[subscription.event_type()].remove(&id);
                                },
                            }
                        }
                    },

                    notification = self.incoming.recv().fuse() => {
                        if let Ok(notification) = notification {
                            // Broadcast the notification...
                            let event = notification.event_type();
                            for (subscription, encoding_set) in plan[event].iter() {
                                // ... by subscription scope
                                if let Some(applied_notification) = notification.apply_subscription(&**subscription) {
                                    for (encoding, connection_set) in encoding_set.iter() {
                                        // ... by message encoding
                                        let message = C::into_message(&applied_notification, encoding);
                                        for (id, connection) in connection_set.iter() {
                                            // ... to listeners connections
                                            match connection.send(message.clone()) {
                                                Ok(_) => {
                                                    trace!("[Broadcaster-{}] sent notification {notification} to listener {id}", self.name);
                                                },
                                                Err(_) => {
                                                    if connection.is_closed() {
                                                        trace!("[Broadcaster-{}] could not send a notification to listener {id} because its connection is closed - removing it", self.name);
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
                            debug!("[Broadcaster-{}] notification stream ended", self.name);
                            let _ = self.shutdown.drain();
                            let _ = self.shutdown.try_send(());
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
        });
    }

    pub fn register(&self, subscription: DynSubscription, id: ListenerId, connection: C) -> Result<()> {
        if subscription.active() {
            self.ctl.try_send(Ctl::Register(subscription, id, connection))?;
        } else {
            self.ctl.try_send(Ctl::Unregister(subscription, id))?;
        }
        Ok(())
    }

    async fn join_notification_broadcasting_task(&self) -> Result<()> {
        trace!("[Broadcaster-{}] joining", self.name);
        self.shutdown.recv().await?;
        debug!("[Broadcaster-{}] terminated", self.name);
        Ok(())
    }

    pub async fn join(&self) -> Result<()> {
        self.join_notification_broadcasting_task().await
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
            overall_test_steps, utxos_changed_test_steps, virtual_chain_changed_test_steps, Step, TestConnection,
        },
    };
    use async_channel::{unbounded, Sender};

    type TestBroadcaster = Broadcaster<TestNotification, ChannelConnection<TestNotification>>;

    struct Test {
        name: &'static str,
        broadcaster: Arc<TestBroadcaster>,
        /// Listeners, vector index = ListenerId
        listeners: Vec<Listener<TestConnection>>,
        ctl_sender: Sender<Ctl<TestConnection>>,
        sync_receiver: Receiver<()>,
        notification_sender: Sender<TestNotification>,
        notification_receivers: Vec<Receiver<TestNotification>>,
        steps: Vec<Step>,
    }

    impl Test {
        fn new(name: &'static str, listener_count: usize, steps: Vec<Step>) -> Self {
            let (sync_sender, sync_receiver) = unbounded();
            let (notification_sender, notification_receiver) = unbounded();
            let broadcaster = Arc::new(TestBroadcaster::new("test", notification_receiver, Some(sync_sender)));
            let mut listeners = Vec::with_capacity(listener_count);
            let mut notification_receivers = Vec::with_capacity(listener_count);
            for _ in 0..listener_count {
                let (sender, receiver) = unbounded();
                let connection = TestConnection::new(sender, ChannelType::Closable);
                let listener = Listener::new(connection);
                listeners.push(listener);
                notification_receivers.push(receiver);
            }
            Self {
                name,
                broadcaster: broadcaster.clone(),
                listeners,
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
            for step in self.steps.iter() {
                // Apply the subscription mutations and register the changes into the broadcaster
                for (idx, mutation) in step.mutations.iter().enumerate() {
                    if let Some(ref mutation) = mutation {
                        let event = mutation.event_type();
                        if self.listeners[idx].subscriptions[event].mutate(mutation.clone()).is_some() {
                            let ctl = match mutation.active() {
                                true => Ctl::Register(
                                    self.listeners[idx].subscriptions[event].clone_arc(),
                                    idx as u64,
                                    self.listeners[idx].connection(),
                                ),
                                false => Ctl::Unregister(self.listeners[idx].subscriptions[event].clone_arc(), idx as u64),
                            };
                            assert!(
                                self.ctl_sender.send(ctl).await.is_ok(),
                                "{} - {}: sending a registration message failed",
                                self.name,
                                step.name
                            );
                            assert!(
                                self.sync_receiver.recv().await.is_ok(),
                                "{} - {}: receiving a sync message failed",
                                self.name,
                                step.name
                            );
                        }
                    }
                }

                // Send the notification
                assert!(
                    self.notification_sender.send_blocking(step.notification.clone()).is_ok(),
                    "{} - {}: sending the notification failed",
                    self.name,
                    step.name
                );
                assert!(self.sync_receiver.recv().await.is_ok(), "{} - {}: receiving a sync message failed", self.name, step.name);

                // Check what the listeners do receive
                for (idx, expected) in step.expected_notifications.iter().enumerate() {
                    if let Some(ref expected) = expected {
                        let notification = self.notification_receivers[idx].recv().await.unwrap();
                        assert_eq!(*expected, notification, "{} - {}: listener[{}] got wrong notification", self.name, step.name, idx);
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
