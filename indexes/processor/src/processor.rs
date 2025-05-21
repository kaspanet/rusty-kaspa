use crate::{
    errors::{IndexError, IndexResult},
    IDENT,
};
use async_trait::async_trait;
use kaspa_consensus_notify::{notification as consensus_notification, notification::Notification as ConsensusNotification};
use kaspa_core::{debug, trace};
use kaspa_index_core::notification::{Notification, PruningPointUtxoSetOverrideNotification, UtxosChangedNotification};
use kaspa_notify::{
    collector::{Collector, CollectorNotificationReceiver},
    error::Result,
    events::EventType,
    notification::Notification as NotificationTrait,
    notifier::DynNotify,
};
use kaspa_utils::triggers::SingleTrigger;
use kaspa_utxoindex::api::UtxoIndexProxy;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Processor processes incoming consensus UtxosChanged and PruningPointUtxoSetOverride
/// notifications submitting them to a UtxoIndex.
///
/// It also acts as a [`Collector`], converting the incoming consensus notifications
/// into their pending local versions and relaying them to a local notifier.
#[derive(Debug)]
pub struct Processor {
    /// An optional UTXO indexer
    utxoindex: Option<UtxoIndexProxy>,

    recv_channel: CollectorNotificationReceiver<ConsensusNotification>,

    /// Has this collector been started?
    is_started: Arc<AtomicBool>,

    collect_shutdown: Arc<SingleTrigger>,
}

impl Processor {
    pub fn new(utxoindex: Option<UtxoIndexProxy>, recv_channel: CollectorNotificationReceiver<ConsensusNotification>) -> Self {
        Self {
            utxoindex,
            recv_channel,
            collect_shutdown: Arc::new(SingleTrigger::new()),
            is_started: Arc::new(AtomicBool::new(false)),
        }
    }

    fn spawn_collecting_task(self: Arc<Self>, notifier: DynNotify<Notification>) {
        // The task can only be spawned once
        if self.is_started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        tokio::spawn(async move {
            trace!("[Index processor] collecting task starting");

            while let Ok(notification) = self.recv_channel.recv().await {
                match self.process_notification(notification).await {
                    Ok(notification) => match notifier.notify(notification) {
                        Ok(_) => (),
                        Err(err) => {
                            trace!("[Index processor] notification sender error: {err:?}");
                        }
                    },
                    Err(err) => {
                        trace!("[Index processor] error while processing a consensus notification: {err:?}");
                    }
                }
            }

            debug!("[Index processor] notification stream ended");
            self.collect_shutdown.trigger.trigger();
            trace!("[Index processor] collecting task ended");
        });
    }

    async fn process_notification(self: &Arc<Self>, notification: ConsensusNotification) -> IndexResult<Notification> {
        match notification {
            ConsensusNotification::UtxosChanged(utxos_changed) => {
                Ok(Notification::UtxosChanged(self.process_utxos_changed(utxos_changed).await?))
            }
            ConsensusNotification::PruningPointUtxoSetOverride(_) => {
                Ok(Notification::PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification {}))
            }
            _ => Err(IndexError::NotSupported(notification.event_type())),
        }
    }

    async fn process_utxos_changed(
        self: &Arc<Self>,
        notification: consensus_notification::UtxosChangedNotification,
    ) -> IndexResult<UtxosChangedNotification> {
        trace!("[{IDENT}]: processing {:?}", notification);
        if let Some(utxoindex) = self.utxoindex.clone() {
            let converted_notification: UtxosChangedNotification =
                utxoindex.update(notification.accumulated_utxo_diff.clone(), notification.virtual_parents).await?.into();
            debug!(
                "IDXPRC, Creating UtxosChanged notifications with {} added and {} removed utxos",
                converted_notification.added.len(),
                converted_notification.removed.len()
            );
            return Ok(converted_notification);
        };
        Err(IndexError::NotSupported(EventType::UtxosChanged))
    }

    async fn join_collecting_task(&self) -> Result<()> {
        trace!("[Index processor] joining");
        self.collect_shutdown.listener.clone().await;
        debug!("[Index processor] terminated");
        Ok(())
    }
}

#[async_trait]
impl Collector<Notification> for Processor {
    fn start(self: Arc<Self>, notifier: DynNotify<Notification>) {
        self.spawn_collecting_task(notifier);
    }

    async fn join(self: Arc<Self>) -> Result<()> {
        self.join_collecting_task().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_channel::{unbounded, Receiver, Sender};
    use kaspa_consensus::{config::Config, consensus::test_consensus::TestConsensus, params::DEVNET_PARAMS, test_helpers::*};
    use kaspa_consensus_core::utxo::{utxo_collection::UtxoCollection, utxo_diff::UtxoDiff};
    use kaspa_consensusmanager::ConsensusManager;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_database::utils::DbLifetime;
    use kaspa_notify::notifier::test_helpers::NotifyMock;
    use kaspa_utxoindex::UtxoIndex;
    use rand::{rngs::SmallRng, SeedableRng};
    use std::sync::Arc;

    // TODO: rewrite with Simnet, when possible.

    #[allow(dead_code)]
    struct NotifyPipeline {
        consensus_sender: Sender<ConsensusNotification>,
        processor: Arc<Processor>,
        processor_receiver: Receiver<Notification>,
        test_consensus: TestConsensus,
        utxoindex_db_lifetime: DbLifetime,
    }

    impl NotifyPipeline {
        fn new() -> Self {
            let (consensus_sender, consensus_receiver) = unbounded();
            let (utxoindex_db_lifetime, utxoindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10)).unwrap();
            let config = Arc::new(Config::new(DEVNET_PARAMS));
            let tc = TestConsensus::new(&config);
            tc.init();
            let consensus_manager = Arc::new(ConsensusManager::from_consensus(tc.consensus_clone()));
            let utxoindex = Some(UtxoIndexProxy::new(UtxoIndex::new(consensus_manager, utxoindex_db).unwrap()));
            let processor = Arc::new(Processor::new(utxoindex, consensus_receiver));
            let (processor_sender, processor_receiver) = unbounded();
            let notifier = Arc::new(NotifyMock::new(processor_sender));
            processor.clone().start(notifier);
            Self { test_consensus: tc, consensus_sender, processor, processor_receiver, utxoindex_db_lifetime }
        }
    }

    #[tokio::test]
    async fn test_utxos_changed_notification() {
        let pipeline = NotifyPipeline::new();
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
        assert!(pipeline.processor_receiver.is_empty(), "the notification receiver should be empty");
        pipeline.consensus_sender.close();
        pipeline.processor.clone().join().await.expect("stopping the processor must succeed");
    }

    #[tokio::test]
    async fn test_pruning_point_utxo_set_override_notification() {
        let pipeline = NotifyPipeline::new();
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
        assert!(pipeline.processor_receiver.is_empty(), "the notification receiver should be empty");
        pipeline.consensus_sender.close();
        pipeline.processor.clone().join().await.expect("stopping the processor must succeed");
    }
}
