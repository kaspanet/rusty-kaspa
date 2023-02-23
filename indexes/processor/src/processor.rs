use crate::{
    errors::{IndexError, IndexResult},
    notification::{Notification, PruningPointUtxoSetOverrideNotification, UtxosChangedNotification},
};
use async_trait::async_trait;
use consensus_notify::{notification as consensus_notification, notification::Notification as ConsensusNotification};
use futures::{
    future::FutureExt, // for `.fuse()`
    select,
};
use kaspa_core::trace;
use kaspa_notify::{
    collector::{Collector, CollectorNotificationReceiver},
    error::{Error, Result},
    events::EventType,
    notification::Notification as NotificationTrait,
    notifier::DynNotify,
};
use kaspa_utils::triggers::DuplexTrigger;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use utxoindex::api::DynUtxoIndexApi;

/// Processor processes incoming consensus UtxosChanged and PruningPointUtxoSetOverride
/// notifications submitting them to a UtxoIndex.
///
/// It also acts as a [`Collector`], converting the incoming consensus notifications
/// into their pending local versions and relaying them to a local notifier.
#[derive(Debug)]
pub struct Processor {
    utxoindex: DynUtxoIndexApi,
    recv_channel: CollectorNotificationReceiver<ConsensusNotification>,

    /// Has this collector been started?
    is_started: Arc<AtomicBool>,

    collect_shutdown: Arc<DuplexTrigger>,
}

impl Processor {
    pub fn new(utxoindex: DynUtxoIndexApi, recv_channel: CollectorNotificationReceiver<ConsensusNotification>) -> Self {
        Self {
            utxoindex,
            recv_channel,
            collect_shutdown: Arc::new(DuplexTrigger::new()),
            is_started: Arc::new(AtomicBool::new(false)),
        }
    }

    fn spawn_collecting_task(self: Arc<Self>, notifier: DynNotify<Notification>) {
        // The task can only be spawned once
        if self.is_started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        tokio::spawn(async move {
            trace!("[Processor] collecting_task start");

            loop {
                select! {
                    _ = self.collect_shutdown.request.listener.clone().fuse() => break,

                    notification = self.recv_channel.recv().fuse() => {
                        match notification {
                            Ok(notification) => {
                                match self.process_notification(notification){
                                    Ok(notification) => {
                                        match notifier.notify(notification) {
                                            Ok(_) => (),
                                            Err(err) => {
                                                trace!("[Processor] notification sender error: {err:?}");
                                            },
                                        }
                                    },
                                    Err(err) => {
                                        trace!("[Processor] error while processing a consensus notification: {err:?}");
                                    }
                                }
                            },
                            Err(err) => {
                                trace!("[Processor] error while receiving a consensus notification: {err:?}");
                            }
                        }
                    }
                }
            }
            self.collect_shutdown.response.trigger.trigger();
            trace!("[Processor] collecting_task end");
        });
    }

    fn process_notification(self: &Arc<Self>, notification: ConsensusNotification) -> IndexResult<Notification> {
        match notification {
            ConsensusNotification::UtxosChanged(utxos_changed) => {
                Ok(Notification::UtxosChanged(self.process_utxos_changed(utxos_changed)?))
            }
            ConsensusNotification::PruningPointUtxoSetOverride(_) => {
                Ok(Notification::PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification {}))
            }
            _ => Err(IndexError::NotSupported(notification.event_type())),
        }
    }

    fn process_utxos_changed(
        self: &Arc<Self>,
        notification: consensus_notification::UtxosChangedNotification,
    ) -> IndexResult<UtxosChangedNotification> {
        if let Some(utxoindex) = self.utxoindex.as_deref() {
            return Ok(utxoindex.write().update(notification.accumulated_utxo_diff.clone(), notification.virtual_parents)?.into());
        };
        Err(IndexError::NotSupported(EventType::UtxosChanged))
    }

    async fn stop_collecting_task(&self) -> Result<()> {
        if self.is_started.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Err(Error::AlreadyStoppedError);
        }
        self.collect_shutdown.request.trigger.trigger();
        self.collect_shutdown.response.listener.clone().await;
        Ok(())
    }
}

#[async_trait]
impl Collector<Notification> for Processor {
    fn start(self: Arc<Self>, notifier: DynNotify<Notification>) {
        self.spawn_collecting_task(notifier);
    }

    async fn stop(self: Arc<Self>) -> Result<()> {
        self.stop_collecting_task().await
    }
}
