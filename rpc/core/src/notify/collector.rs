use async_std::channel::{Receiver, Sender};
use async_std::stream::StreamExt;
use async_trait::async_trait;
use core::fmt::Debug;
use futures::{
    future::FutureExt, // for `.fuse()`
    pin_mut,
    select,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
extern crate derive_more;
use crate::notify::{collector, notifier::Notifier, result::Result};
use crate::Notification;
use derive_more::Deref;
use kaspa_utils::channel::Channel;
use kaspa_utils::triggers::DuplexTrigger;

pub type CollectorNotificationChannel<T> = Channel<Arc<T>>;
pub type CollectorNotificationSender<T> = Sender<Arc<T>>;
pub type CollectorNotificationReceiver<T> = Receiver<Arc<T>>;

#[async_trait]
pub trait Collector: Send + Sync + Debug {
    fn start(self: Arc<Self>, notifier: Arc<Notifier>);
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

/// A notifications collector that receives [`T`] from a channel,
/// converts it into a [Notification] and sends it to a its
/// [Notifier].
#[derive(Debug)]
pub struct CollectorFrom<T>
where
    T: Send + Sync + 'static + Sized,
{
    recv_channel: CollectorNotificationReceiver<T>,
    collect_shutdown: Arc<DuplexTrigger>,
    collect_is_running: Arc<AtomicBool>,
}

impl<T> CollectorFrom<T>
where
    T: Send + Sync + 'static + Sized + Debug,
    ArcConvert<T>: Into<Arc<Notification>>,
{
    pub fn new(recv_channel: CollectorNotificationReceiver<T>) -> Self {
        Self { recv_channel, collect_shutdown: Arc::new(DuplexTrigger::new()), collect_is_running: Arc::new(AtomicBool::new(false)) }
    }

    fn start_collect(&self, notifier: Arc<Notifier>) {
        if !self.collect_is_running.load(Ordering::SeqCst) {
            self.collect_task(notifier);
        }
    }

    fn collect_task(&self, notifier: Arc<Notifier>) {
        let collect_shutdown = self.collect_shutdown.clone();
        let collect_is_running = self.collect_is_running.clone();
        let recv_channel = self.recv_channel.clone();
        collect_is_running.store(true, Ordering::SeqCst);

        workflow_core::task::spawn(async move {
            println!("[Collector] collect_task start");

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
                                //let notification_type: crate::NotificationType = (&*rpc_notification).into();
                                //println!("[Collector] collect_task received {:?}", notification_type);
                                match notifier.clone().notify(rpc_notification) {
                                    Ok(_) => (),
                                    Err(err) => {
                                        println!("[Collector] notification sender error: {:?}", err);
                                    },
                                }
                            },
                            None => {
                                println!("[Collector] notifications returned None. This should never happen");
                            }
                        }
                    }
                }
            }
            collect_is_running.store(false, Ordering::SeqCst);
            collect_shutdown.response.trigger.trigger();
            println!("[Collector] collect_task end");
        });
    }

    async fn stop_collect(&self) -> Result<()> {
        if self.collect_is_running.load(Ordering::SeqCst) {
            self.collect_shutdown.request.trigger.trigger();
            self.collect_shutdown.response.listener.clone().await;
        }
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
        self.start_collect(notifier);
    }

    async fn stop(self: Arc<Self>) -> Result<()> {
        self.stop_collect().await
    }
}

/// A rpc_core notification collector providing a simple pass-through.
/// No conversion occurs since both source and target data are of
/// type [`Notification`].
pub type RpcCoreCollector = CollectorFrom<Notification>;
