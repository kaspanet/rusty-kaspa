extern crate derive_more;
use super::{
    connection::Connection,
    error::{Error, Result},
    events::EventArray,
    listener::ListenerId,
    notification::Notification,
    subscription::DynSubscription,
};
use async_channel::Receiver;
use core::fmt::Debug;
use derive_more::Deref;
use futures::{
    future::FutureExt, // for `.fuse()`
    select,
};
use kaspa_core::trace;
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
    Shutdown,
}

#[derive(Debug)]
pub struct Broadcaster<N, C>
where
    N: Notification,
    C: Connection,
{
    name: &'static str,
    started: Arc<AtomicBool>,
    ctl: Channel<Ctl<C>>,
    incoming: Receiver<N>,
    shutdown: Channel<()>,
}

impl<N, C> Broadcaster<N, C>
where
    N: Notification,
    C: Connection<Notification = N>,
{
    pub fn new(name: &'static str, incoming: Receiver<N>) -> Self {
        Self { name, started: Arc::new(AtomicBool::default()), ctl: Channel::unbounded(), incoming, shutdown: Channel::oneshot() }
    }

    pub fn start(self: &Arc<Self>) {
        self.clone().spawn_notification_broadcasting_task();
    }

    fn spawn_notification_broadcasting_task(self: Arc<Self>) {
        // The task can only be spawned once
        if self.started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        trace!("Starting notification broadcasting task");
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
                                Ctl::Shutdown => {
                                    let _ = self.shutdown.drain();
                                    let _ = self.shutdown.try_send(());
                                    break;
                                }
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
                                                    trace!("[Notifier-{}] broadcasting task sent notification {notification} to listener {id}", self.name);
                                                },
                                                Err(_) => {
                                                    if connection.is_closed() {
                                                        trace!("[Notifier-{}] broadcasting task could not send a notification to listener {id} because its connection is closed - removing it", self.name);
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
                        }
                    }
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

    async fn stop_notification_broadcasting_task(&self) -> Result<()> {
        if self.started.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Err(Error::AlreadyStoppedError);
        }
        self.ctl.try_send(Ctl::Shutdown)?;
        self.shutdown.recv().await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.stop_notification_broadcasting_task().await
    }
}
