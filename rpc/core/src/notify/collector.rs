use async_std::channel::{Receiver, Sender};
use async_std::stream::StreamExt;
use async_trait::async_trait;
use core::fmt::Debug;
use futures::{
    future::FutureExt, // for `.fuse()`
    pin_mut,
    select,
};
use kaspa_core::trace;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
extern crate derive_more;
use crate::notify::{collector, errors::Error, notifier::Notifier, result::Result};
use crate::Notification;
use derive_more::Deref;
use kaspa_utils::channel::Channel;
use kaspa_utils::triggers::DuplexTrigger;

pub type CollectorNotificationChannel<T> = Channel<Arc<T>>;
pub type CollectorNotificationSender<T> = Sender<Arc<T>>;
pub type CollectorNotificationReceiver<T> = Receiver<Arc<T>>;

/// A notification collector, acting as a notification source for a [`Notifier`].
///
/// A [`Collector`] is responsible for collecting notifications of
/// a specific form from a specific source, convert them if necessary
/// into [`Notification`]s and forward them to the [Notifier] provided
/// to `Collector::start`.
#[async_trait]
pub trait Collector: Send + Sync + Debug {
    /// Start collecting notifications for `nofifier`
    fn start(self: Arc<Self>, notifier: Arc<Notifier>);
    /// Stop collecting notifications
    async fn stop(self: Arc<Self>) -> Result<()>;
}

pub type DynCollector = Arc<dyn Collector>;

/// A newtype allowing conversion from Arc<T> to Arc<Notification>.
/// See [`super::collector::CollectorFrom`]
#[derive(Clone, Debug, Deref)]
pub struct ArcConvert<T>(Arc<T>);

impl<T> From<Arc<T>> for ArcConvert<T> {
    fn from(item: Arc<T>) -> Self {
        ArcConvert(item)
    }
}

/// A notification [`Collector`] that receives [`T`] from a channel,
/// converts it into a [`Notification`] and sends it to a its
/// [`Notifier`].
#[derive(Debug)]
pub struct CollectorFrom<T>
where
    T: Send + Sync + 'static + Sized,
{
    recv_channel: CollectorNotificationReceiver<T>,

    /// Has this collector been started?
    is_started: Arc<AtomicBool>,

    collect_shutdown: Arc<DuplexTrigger>,
}

impl<T> CollectorFrom<T>
where
    T: Send + Sync + 'static + Sized + Debug,
    ArcConvert<T>: Into<Arc<Notification>>,
{
    pub fn new(recv_channel: CollectorNotificationReceiver<T>) -> Self {
        Self { recv_channel, collect_shutdown: Arc::new(DuplexTrigger::new()), is_started: Arc::new(AtomicBool::new(false)) }
    }

    fn spawn_collecting_task(self: Arc<Self>, notifier: Arc<Notifier>) {
        // The task can only be spawned once
        if self.is_started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) != Ok(false) {
            return;
        }
        let collect_shutdown = self.collect_shutdown.clone();
        let recv_channel = self.recv_channel.clone();

        workflow_core::task::spawn(async move {
            trace!("[Collector] collecting_task start");

            let shutdown = collect_shutdown.request.listener.clone().fuse();
            pin_mut!(shutdown);

            let notifications = recv_channel.fuse();
            pin_mut!(notifications);

            loop {
                select! {
                    _ = shutdown => { break; }
                    notification = notifications.next().fuse() => {
                        match notification {
                            Some(msg) => {
                                let rpc_notification: Arc<Notification> = ArcConvert::from(msg.clone()).into();
                                match notifier.clone().notify(rpc_notification) {
                                    Ok(_) => (),
                                    Err(err) => {
                                        trace!("[Collector] notification sender error: {:?}", err);
                                    },
                                }
                            },
                            None => {
                                trace!("[Collector] notifications returned None. This should never happen");
                            }
                        }
                    }
                }
            }
            collect_shutdown.response.trigger.trigger();
            trace!("[Collector] collecting_task end");
        });
    }

    async fn stop_collecting_task(self: Arc<Self>) -> Result<()> {
        if self.is_started.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst) != Ok(true) {
            return Err(Error::AlreadyStoppedError);
        }
        self.collect_shutdown.request.trigger.trigger();
        self.collect_shutdown.response.listener.clone().await;
        Ok(())
    }
}

#[async_trait]
impl<T> collector::Collector for CollectorFrom<T>
where
    T: Send + Sync + 'static + Sized + Debug,
    ArcConvert<T>: Into<Arc<Notification>>,
{
    fn start(self: Arc<Self>, notifier: Arc<Notifier>) {
        self.spawn_collecting_task(notifier);
    }

    async fn stop(self: Arc<Self>) -> Result<()> {
        self.stop_collecting_task().await
    }
}

/// A rpc_core notification collector providing a simple pass-through.
/// No conversion occurs since both source and target data are of
/// type [`Notification`].
pub type RpcCoreCollector = CollectorFrom<Notification>;
