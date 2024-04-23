use super::{error::Result, notification::Notification};
use crate::{converter::Converter, notifier::DynNotify};
use async_channel::{Receiver, Sender};
use async_trait::async_trait;
use core::fmt::Debug;
use kaspa_core::{debug, trace};
use kaspa_utils::{channel::Channel, triggers::SingleTrigger};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub type CollectorNotificationChannel<T> = Channel<T>;
pub type CollectorNotificationSender<T> = Sender<T>;
pub type CollectorNotificationReceiver<T> = Receiver<T>;

/// A notification collector, relaying notifications to a [`Notifier`](crate::notifier::Notifier).
///
/// A collector is responsible for collecting notifications of a specific form from a specific source,
/// convert them if necessary into `N`s and forward them to the [`Notifier`](crate::notifier::Notifier) provided
/// to `Collector::start`.
#[async_trait]
pub trait Collector<N>: Send + Sync + Debug
where
    N: Notification,
{
    /// Start collecting notifications for `notifier`
    fn start(self: Arc<Self>, notifier: DynNotify<N>);
    /// Join notification collecting tasks. Assumes that the notification system is already
    /// folding up by propagating channel closing from the notification root
    async fn join(self: Arc<Self>) -> Result<()>;
}

pub type DynCollector<N> = Arc<dyn Collector<N>>;

/// A notification [`Collector`] that receives `I` from a channel,
/// converts it into a `N` and sends it to a [`DynNotify<N>`].
#[derive(Debug)]
pub struct CollectorFrom<C>
where
    C: Converter,
{
    name: &'static str,
    recv_channel: CollectorNotificationReceiver<C::Incoming>,
    converter: Arc<C>,

    /// Has this collector been started?
    is_started: Arc<AtomicBool>,

    /// Triggers when the collecting task exits
    collect_shutdown: Arc<SingleTrigger>,
}

impl<C> CollectorFrom<C>
where
    C: Converter + 'static,
{
    pub fn new(name: &'static str, recv_channel: CollectorNotificationReceiver<C::Incoming>, converter: Arc<C>) -> Self {
        Self {
            name,
            recv_channel,
            converter,
            collect_shutdown: Arc::new(SingleTrigger::new()),
            is_started: Arc::new(AtomicBool::new(false)),
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
        let collect_shutdown = self.collect_shutdown.clone();
        let recv_channel = self.recv_channel.clone();
        let converter = self.converter.clone();

        workflow_core::task::spawn(async move {
            trace!("[Collector {}] collecting task starting", self.name);

            while let Ok(notification) = recv_channel.recv().await {
                match notifier.notify(converter.convert(notification).await) {
                    Ok(_) => (),
                    Err(err) => {
                        trace!("[Collector {}] notification sender error: {}", self.name, err);
                    }
                }
            }

            debug!("[Collector {}] notification stream ended", self.name);
            collect_shutdown.trigger.trigger();
            trace!("[Collector {}] collecting task ended", self.name);
        });
    }

    async fn join_collecting_task(self: &Arc<Self>) -> Result<()> {
        trace!("[Collector {}] joining", self.name);
        self.collect_shutdown.listener.clone().await;
        debug!("[Collector {}] terminated", self.name);
        Ok(())
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

    async fn join(self: Arc<Self>) -> Result<()> {
        self.join_collecting_task().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        converter::ConverterFrom,
        events::EventType,
        notifier::test_helpers::NotifyMock,
        subscription::{
            context::SubscriptionContext,
            single::{OverallSubscription, UtxosChangedSubscription, VirtualChainChangedSubscription},
        },
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
        fn apply_overall_subscription(&self, _: &OverallSubscription, _: &SubscriptionContext) -> Option<Self> {
            unimplemented!()
        }

        fn apply_virtual_chain_changed_subscription(
            &self,
            _: &VirtualChainChangedSubscription,
            _: &SubscriptionContext,
        ) -> Option<Self> {
            unimplemented!()
        }

        fn apply_utxos_changed_subscription(&self, _: &UtxosChangedSubscription, _: &SubscriptionContext) -> Option<Self> {
            unimplemented!()
        }

        fn event_type(&self) -> EventType {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_collector_from() {
        type TestConverter = ConverterFrom<IncomingNotification, OutgoingNotification>;
        let incoming = Channel::default();
        let collector: Arc<CollectorFrom<TestConverter>> =
            Arc::new(CollectorFrom::new("test", incoming.receiver(), Arc::new(TestConverter::new())));
        let outgoing = Channel::default();
        let notifier = Arc::new(NotifyMock::new(outgoing.sender()));
        collector.clone().start(notifier);

        assert!(incoming.send(IncomingNotification::A).await.is_ok());
        assert!(incoming.send(IncomingNotification::B).await.is_ok());
        assert!(incoming.send(IncomingNotification::A).await.is_ok());

        assert_eq!(outgoing.recv().await.unwrap(), OutgoingNotification::A);
        assert_eq!(outgoing.recv().await.unwrap(), OutgoingNotification::B);
        assert_eq!(outgoing.recv().await.unwrap(), OutgoingNotification::A);

        incoming.close();
        assert!(collector.join().await.is_ok());
    }
}
