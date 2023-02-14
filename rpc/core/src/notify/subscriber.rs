use async_trait::async_trait;
use core::fmt::Debug;
use kaspa_core::trace;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
extern crate derive_more;
use super::{
    error::{Error, Result},
    listener::ListenerId,
    scope::Scope,
    subscription::Mutation,
};
use crate::{api::ops::SubscribeCommand, RpcResult};
use futures::{
    future::FutureExt, // for `.fuse()`
    select,
};
use workflow_core::channel::Channel;

/// A manager of subscriptions to notifications for registered listeners
#[async_trait]
pub trait SubscriptionManager: Send + Sync + Debug {
    async fn start_notify(self: Arc<Self>, id: ListenerId, scope: Scope) -> RpcResult<()>;
    async fn stop_notify(self: Arc<Self>, id: ListenerId, scope: Scope) -> RpcResult<()>;

    async fn execute_subscribe_command(self: Arc<Self>, id: ListenerId, scope: Scope, command: SubscribeCommand) -> RpcResult<()> {
        match command {
            SubscribeCommand::Start => self.start_notify(id, scope).await,
            SubscribeCommand::Stop => self.stop_notify(id, scope).await,
        }
    }
}

pub type DynSubscriptionManager = Arc<dyn SubscriptionManager>;

#[derive(Clone, Debug)]
enum Ctl {
    Shutdown,
}

/// A subscriber handling subscription messages executing them into a [SubscriptionManager].
#[derive(Debug)]
pub struct Subscriber {
    /// Subscription manager
    subscription_manager: DynSubscriptionManager,

    /// Listener ID
    listener_id: ListenerId,

    /// Has this subscriber been started?
    started: Arc<AtomicBool>,

    ctl: Channel<Ctl>,
    incoming: Channel<Mutation>,
    shutdown: Channel<()>,
}

impl Subscriber {
    pub fn new(subscription_manager: DynSubscriptionManager, listener_id: ListenerId) -> Self {
        Self {
            subscription_manager,
            listener_id,
            started: Arc::new(AtomicBool::default()),
            ctl: Channel::unbounded(),
            incoming: Channel::unbounded(),
            shutdown: Channel::oneshot(),
        }
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
        trace!("Starting notification broadcasting task");
        workflow_core::task::spawn(async move {
            loop {
                select! {
                    ctl = self.ctl.recv().fuse() => {
                        if let Ok(ctl) = ctl {
                            match ctl {
                                Ctl::Shutdown => {
                                    let _ = self.shutdown.drain();
                                    let _ = self.shutdown.try_send(());
                                    break;
                                }
                            }
                        }
                    },

                    mutation = self.incoming.recv().fuse() => {
                        if let Ok(mutation) = mutation {
                            if let Err(err) = self.subscription_manager.clone().execute_subscribe_command(self.listener_id, mutation.scope, mutation.command).await {
                                trace!("[Subscriber] the subscription command returned an error: {:?}", err);
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn mutate(self: &Arc<Self>, mutation: Mutation) -> Result<()> {
        self.incoming.try_send(mutation)?;
        Ok(())
    }

    async fn stop_subscription_receiver_task(self: &Arc<Self>) -> Result<()> {
        if self.started.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Err(Error::AlreadyStoppedError);
        }
        self.ctl.try_send(Ctl::Shutdown)?;
        self.shutdown.recv().await?;
        Ok(())
    }

    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        self.stop_subscription_receiver_task().await
    }
}
