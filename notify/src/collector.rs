use super::{error::Result, notification::Notification};
use crate::{converter::Converter, notifier::DynNotify};
use async_channel::{Receiver, Sender};
use async_trait::async_trait;
use core::fmt::Debug;
use parking_lot::Mutex;
use triggered::Listener as TriggerListener;

use kaspa_core::trace;
use kaspa_utils::channel::Channel;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub type CollectorNotificationChannel<T> = Channel<T>;
pub type CollectorNotificationSender<T> = Sender<T>;
pub type CollectorNotificationReceiver<T> = Receiver<T>;

/// A notification collector, relaying notifications to a [`Notifier`].
///
/// A [`Collector`] is responsible for collecting notifications of
/// a specific form from a specific source, convert them if necessary
/// into `N`s and forward them to the [Notifier] provided
/// to `Collector::start`.
#[async_trait]
pub trait Collector<N>: Send + Sync + Debug
where
    N: Notification,
{
    /// Start collecting notifications for `notifier`
    fn start(self: Arc<Self>, notifier: DynNotify<N>);
    /// Stop collecting notifications
    async fn stop(self: Arc<Self>) -> Result<()>;
}

pub type DynCollector<N> = Arc<dyn Collector<N>>;

/// A notification [`Collector`] that receives `I` from a channel,
/// converts it into a `N` and sends it to a [`DynNotify<N>`].
#[derive(Debug)]
pub struct CollectorFrom<C>
where
    C: Converter,
{
    incoming: Receiver<C::Incoming>,

    converter: Arc<C>,

    /// Has this collector been started?
    is_started: Arc<AtomicBool>,

    /// Has this collector been closed?
    is_closed: Arc<AtomicBool>,

    /// Shutdown wait
    shutdown_waits: Arc<Mutex<Vec<TriggerListener>>>,
}

impl<C> CollectorFrom<C>
where
    C: Converter + 'static,
{
    pub fn new(incoming: Receiver<C::Incoming>, converter: Arc<C>) -> Self {
        Self {
            incoming,
            converter,
            is_started: Arc::new(AtomicBool::new(false)),
            is_closed: Arc::new(AtomicBool::new(false)),
            shutdown_waits: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn is_started(&self) -> bool {
        self.is_started.load(Ordering::SeqCst)
    }

    fn spawn_collecting_task(self: Arc<Self>, notifier: DynNotify<C::Outgoing>) {
        // The task can only be spawned once
        if self.is_started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        let converter = self.converter.clone();
        let notifier_ident = notifier.ident();

        workflow_core::task::spawn(async move {
            trace!("[Collector for {0}] collecting task start", notifier_ident);

            loop {
                let notification = self.incoming.recv().await;
                if let Ok(notification) = notification {
                    match notifier.notify(converter.convert(notification).await) {
                        Ok(_) => (),
                        Err(err) => {
                            trace!("[Collector for {0}] notification sender error: {1}", notifier_ident, err);
                        }
                    };
                } else if let Err(err) = notification {
                    // We did not expect to exit...
                    if !self.is_closed.load(Ordering::SeqCst) {
                        panic!("[Collector for {0}] notifications error: {1}", notifier_ident, err);
                    }
                    break;
                };
            }
        });
    }

    async fn stop_collecting_task(self: &Arc<Self>) -> Result<()> {
        if self.is_closed.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            trace!("[Collector] stopping");
            self.incoming.close();
            let waits = self.shutdown_waits.lock().clone();
            for l in waits.into_iter() {
                l.await
            }
        }
        trace!("[Collector] already stopped");
        Ok(())
    }

    #[cfg(test)]
    pub fn is_closed(&self) -> bool {
        self.is_closed.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl<N, C> Collector<N> for CollectorFrom<C>
where
    N: Notification,
    C: Converter<Outgoing = N> + 'static,
{
    fn start(self: Arc<Self>, notifier: DynNotify<N>) {
        self.spawn_collecting_task(notifier);
    }

    async fn stop(self: Arc<Self>) -> Result<()> {
        self.stop_collecting_task().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        converter::ConverterFrom,
        events::EventType,
        notifier::test_helpers::NotifyMock,
        subscription::single::{OverallSubscription, UtxosChangedSubscription, VirtualChainChangedSubscription},
    };
    use derive_more::Display;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum IncomingNotification {
        A,
        B,
    }

    #[derive(Clone, Debug, Display, PartialEq, Eq)]
    enum OutgoingNotification {
        A,
        B,
    }

    impl From<IncomingNotification> for OutgoingNotification {
        fn from(value: IncomingNotification) -> Self {
            match value {
                IncomingNotification::A => OutgoingNotification::A,
                IncomingNotification::B => OutgoingNotification::B,
            }
        }
    }

    impl crate::notification::Notification for OutgoingNotification {
        fn apply_overall_subscription(&self, _: &OverallSubscription) -> Option<Self> {
            unimplemented!()
        }

        fn apply_virtual_chain_changed_subscription(&self, _: &VirtualChainChangedSubscription) -> Option<Self> {
            unimplemented!()
        }

        fn apply_utxos_changed_subscription(&self, _: &UtxosChangedSubscription) -> Option<Self> {
            unimplemented!()
        }

        fn event_type(&self) -> EventType {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_collector_from() {
        type TestConverter = ConverterFrom<IncomingNotification, OutgoingNotification>;
        let incoming_channel = Channel::default();
        let collector: Arc<CollectorFrom<TestConverter>> =
            Arc::new(CollectorFrom::new(incoming_channel.receiver(), Arc::new(TestConverter::new())));
        let outgoing_channel = Channel::default();
        let notifier = Arc::new(NotifyMock::new(outgoing_channel.sender(), "notifier_mock"));
        collector.clone().start(notifier);

        assert!(incoming_channel.sender().send(IncomingNotification::A).await.is_ok());
        assert!(incoming_channel.sender().send(IncomingNotification::B).await.is_ok());
        assert!(incoming_channel.sender().send(IncomingNotification::A).await.is_ok());

        assert_eq!(outgoing_channel.receiver().recv().await.unwrap(), OutgoingNotification::A);
        assert_eq!(outgoing_channel.receiver().recv().await.unwrap(), OutgoingNotification::B);
        assert_eq!(outgoing_channel.receiver().recv().await.unwrap(), OutgoingNotification::A);

        assert!(collector.clone().stop().await.is_ok());
        assert!(collector.is_closed(), "collector is not closed");
        assert!(incoming_channel.receiver().is_closed(), "collector receiver is not closed");
        assert!(incoming_channel.receiver().is_empty(), "collector stopped with none empty receiver");
        assert!(collector.stop().await.is_ok(), "collector shouldn't error when re-stopping");
    }
}
