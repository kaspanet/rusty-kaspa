use crate::{
    errors::{IndexError, IndexResult},
    IDENT,
};
use async_trait::async_trait;
use kaspa_consensus_notify::{notification as consensus_notification, notification::Notification as ConsensusNotification};
use kaspa_core::trace;
use kaspa_index_core::notification::{
    ConsensusShutdownNotification, Notification, PruningPointUtxoSetOverrideNotification, UtxosChangedNotification,
};
use kaspa_notify::{
    collector::{Collector, CollectorNotificationReceiver},
    error::Result,
    events::EventType,
    notification::Notification as NotificationTrait,
    notifier::DynNotify,
};
use kaspa_utxoindex::api::DynUtxoIndexApi;
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use triggered::{trigger, Listener as TriggeredListener};

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

    /// Has this collector been closed?
    is_closed: Arc<AtomicBool>,

    shutdown_waits: Arc<Mutex<Vec<TriggeredListener>>>,
}

impl Processor {
    pub fn new(utxoindex: DynUtxoIndexApi, recv_channel: CollectorNotificationReceiver<ConsensusNotification>) -> Self {
        Self {
            utxoindex,
            recv_channel,
            shutdown_waits: Arc::new(Mutex::new(Vec::new())),
            is_started: Arc::new(AtomicBool::new(false)),
            is_closed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn spawn_collecting_task(self: Arc<Self>, notifier: DynNotify<Notification>) {
        // The task can only be spawned once
        if self.is_started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        let (trig, lis) = trigger();
        self.shutdown_waits.lock().push(lis);

        tokio::spawn(async move {
            trace!("[Processor] collecting_task start");

            loop {
                match self.recv_channel.recv().await {
                    Ok(notification) => match self.process_notification(notification) {
                        Ok(notification) => match notifier.notify(notification) {
                            Ok(_) => (),
                            Err(err) => {
                                trace!("[{0}] notification sender error: {1:?}", IDENT, err);
                            }
                        },
                        Err(err) => {
                            trace!("[{0}] error while processing a consensus notification: {1:?}", IDENT, err);
                        }
                    },
                    Err(err) => {
                        // If we do not expect to exit...
                        if !self.is_closed.load(Ordering::SeqCst) {
                            panic!("[{0}] processing error: {1}", IDENT, err);
                        };

                        // Signal shutdown is finished to waiting threads
                        trig.trigger();
                        println!("triggered");

                        // Break out of the loop select
                        break;
                    }
                }
            }
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
            ConsensusNotification::ConsensusShutdown(consensus_shutdown_notfification) => {
                self.process_consensus_shutdown(consensus_shutdown_notfification).unwrap();
                Ok(Notification::ConsensusShutdown(ConsensusShutdownNotification {}))
            }
            _ => Err(IndexError::NotSupported(notification.event_type())),
        }
    }

    fn process_utxos_changed(
        self: &Arc<Self>,
        notification: consensus_notification::UtxosChangedNotification,
    ) -> IndexResult<UtxosChangedNotification> {
        trace!("[{IDENT}]: processing {:?}", notification);
        if let Some(utxoindex) = self.utxoindex.as_deref() {
            return Ok(utxoindex.write().update(notification.accumulated_utxo_diff.clone(), notification.virtual_parents)?.into());
        };
        Err(IndexError::NotSupported(EventType::UtxosChanged))
    }

    fn process_consensus_shutdown(
        self: &Arc<Self>,
        notification: consensus_notification::ConsensusShutdownNotification,
    ) -> Result<()> {
        trace!("[{IDENT}]: processing {:?}", notification);
        tokio::spawn(self.clone().stop());
        Ok(())
    }

    pub async fn stop_collecting_task(&self) -> Result<()> {
        if self.is_closed.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            trace!("[{0}] stopping...", IDENT);
            self.recv_channel.close();
            let waits = self.shutdown_waits.lock().clone();
            for l in waits.into_iter() {
                l.await;
            };
            return Ok(());
        };
        trace!("[{0}] already stopped", IDENT);
        Ok(())
    }

    #[cfg(test)]
    pub fn is_closed(&self) -> bool {
        self.is_closed.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub async fn shutdown_wait(&self) {
        println!("waiting...");
        let waits = self.shutdown_waits.lock().clone();
        for l in waits.into_iter() {
            l.await;
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_channel::{unbounded, Receiver, Sender};
    use kaspa_consensus::{config::Config, consensus::test_consensus::TestConsensus, params::DEVNET_PARAMS, test_helpers::*};
    use kaspa_consensus_core::utxo::{utxo_collection::UtxoCollection, utxo_diff::UtxoDiff};
    use kaspa_consensusmanager::ConsensusManager;
    use kaspa_database::utils::{create_temp_db, DbLifetime};
    use kaspa_notify::notifier::test_helpers::NotifyMock;
    use kaspa_utxoindex::{api::DynUtxoIndexApi, UtxoIndex};
    use rand::{rngs::SmallRng, SeedableRng};
    use std::sync::Arc;

    // TODO: rewrite with Simnet, when possible.

    #[allow(dead_code)]
    struct NotifyPipeline {
        consensus_sender: Sender<ConsensusNotification>,
        processor: Arc<Processor>,
        notifier: Arc<NotifyMock<Notification>>,
        processor_receiver: Receiver<Notification>,
        test_consensus: TestConsensus,
        utxoindex_db_lifetime: DbLifetime,
    }

    impl NotifyPipeline {
        fn new() -> Self {
            let (consensus_sender, consensus_receiver) = unbounded();
            let (utxoindex_db_lifetime, utxoindex_db) = create_temp_db();
            let config = Arc::new(Config::new(DEVNET_PARAMS));
            let tc = TestConsensus::new(&config);
            tc.init();
            let consensus_manager = Arc::new(ConsensusManager::from_consensus(tc.consensus_clone()));
            let utxoindex: DynUtxoIndexApi = Some(UtxoIndex::new(consensus_manager, utxoindex_db).unwrap());
            let processor = Arc::new(Processor::new(utxoindex, consensus_receiver));
            let (processor_sender, processor_receiver) = unbounded::<Notification>();
            let notifier = Arc::new(NotifyMock::new(processor_sender, "mock_notfier"));
            Self { test_consensus: tc, consensus_sender, processor, notifier, processor_receiver, utxoindex_db_lifetime }
        }

        fn start(&self) {
            self.processor.clone().start(self.notifier.clone());
        }

        async fn stop(&self) -> Result<()> {
            self.processor.clone().stop().await
        }
    }

    #[tokio::test]
    async fn test_utxos_changed_notification() {
        kaspa_core::log::try_init_logger("trace, kaspa-index-processor=trace");
        let pipeline = NotifyPipeline::new();
        pipeline.start();
        let rng = &mut SmallRng::seed_from_u64(42);

        let mut to_add_collection = UtxoCollection::new();
        let mut to_remove_collection = UtxoCollection::new();
        for _ in 0..2 {
            to_add_collection.insert(generate_random_outpoint(rng), generate_random_utxo(rng));
            to_remove_collection.insert(generate_random_outpoint(rng), generate_random_utxo(rng));
        }

        let test_notification = consensus_notification::UtxosChangedNotification::new(
            Arc::new(UtxoDiff { add: to_add_collection, remove: to_remove_collection }),
            Arc::new(generate_random_hashes(rng, 2)),
        );

        pipeline.consensus_sender.send(ConsensusNotification::UtxosChanged(test_notification.clone())).await.expect("expected send");

        match pipeline.processor_receiver.recv().await.expect("receives a notification") {
            Notification::UtxosChanged(utxo_changed_notification) => {
                let mut notification_utxo_added_count = 0;
                for (script_public_key, compact_utxo_collection) in utxo_changed_notification.added.iter() {
                    for (transaction_outpoint, compact_utxo) in compact_utxo_collection.iter() {
                        let test_utxo = test_notification
                            .accumulated_utxo_diff
                            .add
                            .get(transaction_outpoint)
                            .expect("expected transaction outpoint to be in test event");
                        assert_eq!(test_utxo.script_public_key, *script_public_key);
                        assert_eq!(test_utxo.amount, compact_utxo.amount);
                        assert_eq!(test_utxo.block_daa_score, compact_utxo.block_daa_score);
                        assert_eq!(test_utxo.is_coinbase, compact_utxo.is_coinbase);
                        notification_utxo_added_count += 1;
                    }
                }
                assert_eq!(test_notification.accumulated_utxo_diff.add.len(), notification_utxo_added_count);

                let mut notification_utxo_removed_count = 0;
                for (script_public_key, compact_utxo_collection) in utxo_changed_notification.removed.iter() {
                    for (transaction_outpoint, compact_utxo) in compact_utxo_collection.iter() {
                        let test_utxo = test_notification
                            .accumulated_utxo_diff
                            .remove
                            .get(transaction_outpoint)
                            .expect("expected transaction outpoint to be in test event");
                        assert_eq!(test_utxo.script_public_key, *script_public_key);
                        assert_eq!(test_utxo.amount, compact_utxo.amount);
                        assert_eq!(test_utxo.block_daa_score, compact_utxo.block_daa_score);
                        assert_eq!(test_utxo.is_coinbase, compact_utxo.is_coinbase);
                        notification_utxo_removed_count += 1;
                    }
                }
                assert_eq!(test_notification.accumulated_utxo_diff.remove.len(), notification_utxo_removed_count);
            }
            unexpected_notification => panic!("Unexpected notification: {unexpected_notification:?}"),
        }
        pipeline.stop().await.expect("stopping the processor must succeed");
        assert!(pipeline.processor.is_closed(), "the processor should be closed");
        assert!(pipeline.processor.recv_channel.is_closed(), "processor receiver is not closed");
        assert!(pipeline.processor.recv_channel.is_empty(), "processor stopped with none empty receiver");
        assert!(pipeline.processor.stop().await.is_ok(), "processor shouldn't error when re-stopping");
    }

    #[tokio::test]
    async fn test_pruning_point_utxo_set_override_notification() {
        kaspa_core::log::try_init_logger("trace, kaspa-index-processor=trace");
        let pipeline = NotifyPipeline::new();
        pipeline.start();
        let test_notification = consensus_notification::PruningPointUtxoSetOverrideNotification {};
        pipeline
            .consensus_sender
            .send(ConsensusNotification::PruningPointUtxoSetOverride(test_notification.clone()))
            .await
            .expect("expected send");
        match pipeline.processor_receiver.recv().await.expect("expected recv") {
            Notification::PruningPointUtxoSetOverride(_) => (),
            unexpected_notification => panic!("Unexpected notification: {unexpected_notification:?}"),
        }
        pipeline.stop().await.expect("stopping the processor must succeed");
        assert!(pipeline.processor.is_closed(), "the processor should be closed");
        assert!(pipeline.processor.recv_channel.is_closed(), "processor receiver is not closed");
        assert!(pipeline.processor.recv_channel.is_empty(), "processor stopped with none empty receiver");
        assert!(pipeline.processor.stop().await.is_ok(), "processor shouldn't error when re-stopping");
    }

    #[tokio::test]
    async fn test_consensus_shutdown_notification() {
        kaspa_core::log::try_init_logger("trace, kaspa-index-processor=trace");
        let pipeline = NotifyPipeline::new();
        pipeline.start();
        let test_notification = consensus_notification::ConsensusShutdownNotification {};
        pipeline
            .consensus_sender
            .send(ConsensusNotification::ConsensusShutdown(test_notification.clone()))
            .await
            .expect("expected send");
        match pipeline.processor_receiver.recv().await.expect("expected recv") {
            Notification::ConsensusShutdown(_) => (),
            unexpected_notification => panic!("Unexpected notification: {unexpected_notification:?}"),
        }
        pipeline.processor.shutdown_wait().await;
        assert!(pipeline.processor.is_closed(), "the processor should be closed");
        assert!(pipeline.processor.recv_channel.is_closed(), "processor receiver is not closed");
        assert!(pipeline.processor.recv_channel.is_empty(), "processor stopped with none empty receiver");
        assert!(pipeline.processor.stop().await.is_ok(), "processor shouldn't error when re-stopping");
    }
}
