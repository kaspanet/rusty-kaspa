use async_channel::{Receiver, Sender};
use async_trait::async_trait;
use core::fmt::Debug;
use kaspa_core::trace;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
extern crate derive_more;
use super::{error::Error, listener::ListenerID, message::SubscribeMessage, result::Result, scope::Scope};
use crate::{api::ops::SubscribeCommand, RpcResult};
use kaspa_utils::channel::Channel;

/// A manager of subscriptions to notifications for registered listeners
#[async_trait]
pub trait SubscriptionManager: Send + Sync + Debug {
    async fn start_notify(self: Arc<Self>, id: ListenerID, notification_type: Scope) -> RpcResult<()>;
    async fn stop_notify(self: Arc<Self>, id: ListenerID, notification_type: Scope) -> RpcResult<()>;

    async fn execute_notify_command(
        self: Arc<Self>,
        id: ListenerID,
        notification_type: Scope,
        command: SubscribeCommand,
    ) -> RpcResult<()> {
        match command {
            SubscribeCommand::Start => self.start_notify(id, notification_type).await,
            SubscribeCommand::Stop => self.stop_notify(id, notification_type).await,
        }
    }
}

pub type DynSubscriptionManager = Arc<dyn SubscriptionManager>;

/// A subscriber handling subscription messages executing them into a [SubscriptionManager].
#[derive(Debug)]
pub struct Subscriber {
    /// Subscription manager
    subscription_manager: DynSubscriptionManager,
    listener_id: ListenerID,

    /// Has this subscriber been started?
    is_started: Arc<AtomicBool>,

    /// Feedback channel
    subscribe_channel: Channel<SubscribeMessage>,
    subscribe_shutdown_listener: Arc<Mutex<Option<triggered::Listener>>>,
}

impl Subscriber {
    pub fn new(subscription_manager: DynSubscriptionManager, listener_id: ListenerID) -> Self {
        Self {
            subscription_manager,
            listener_id,
            subscribe_channel: Channel::default(),
            subscribe_shutdown_listener: Arc::new(Mutex::new(None)),
            is_started: Arc::new(AtomicBool::default()),
        }
    }

    pub(crate) fn sender(&self) -> Sender<SubscribeMessage> {
        self.subscribe_channel.sender()
    }

    pub fn start(self: &Arc<Self>) {
        let (shutdown_trigger, shutdown_listener) = triggered::trigger();
        let mut subscribe_shutdown_listener = self.subscribe_shutdown_listener.lock().unwrap();
        *subscribe_shutdown_listener = Some(shutdown_listener);
        self.spawn_subscription_receiver_task(shutdown_trigger, self.subscribe_channel.receiver());
    }

    /// Launch the subscription receiver
    fn spawn_subscription_receiver_task(&self, shutdown_trigger: triggered::Trigger, subscribe_rx: Receiver<SubscribeMessage>) {
        // The task can only be spawned once
        if self.is_started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        let subscription_manager = self.subscription_manager.clone();
        let listener_id = self.listener_id;

        workflow_core::task::spawn(async move {
            loop {
                let subscribe = subscribe_rx.recv().await.unwrap();

                match subscribe {
                    SubscribeMessage::StartEvent(ref notification_type) => {
                        match subscription_manager.clone().start_notify(listener_id, notification_type.clone()).await {
                            Ok(_) => (),
                            Err(err) => {
                                trace!("[Reporter] start notify error: {:?}", err);
                            }
                        }
                    }

                    SubscribeMessage::StopEvent(ref notification_type) => {
                        match subscription_manager.clone().stop_notify(listener_id, notification_type.clone()).await {
                            Ok(_) => (),
                            Err(err) => {
                                trace!("[Reporter] start notify error: {:?}", err);
                            }
                        }
                    }

                    SubscribeMessage::Shutdown => {
                        break;
                    }
                }
            }
            shutdown_trigger.trigger();
        });
    }

    fn try_send_subscribe(self: &Arc<Self>, msg: SubscribeMessage) -> Result<()> {
        self.subscribe_channel.sender().try_send(msg)?;
        Ok(())
    }

    async fn stop_subscription_receiver_task(self: &Arc<Self>) -> Result<()> {
        if self.is_started.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Err(Error::AlreadyStoppedError);
        }
        let mut result: Result<()> = Ok(());
        match self.try_send_subscribe(SubscribeMessage::Shutdown) {
            Ok(_) => {
                let shutdown_listener: triggered::Listener;
                {
                    let mut subscribe_shutdown_listener = self.subscribe_shutdown_listener.lock().unwrap();
                    shutdown_listener = subscribe_shutdown_listener.take().unwrap();
                }
                shutdown_listener.await;
            }
            Err(err) => result = Err(err),
        }
        result
    }

    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        self.stop_subscription_receiver_task().await
    }
}
