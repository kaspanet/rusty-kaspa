use crate::{
    IDENT,
    errors::{IndexError, IndexResult},
};
use async_trait::async_trait;
use kaspa_consensus_notify::{notification as consensus_notification, notification::Notification as ConsensusNotification};
use kaspa_core::{debug, info, trace};
use kaspa_index_core::notification::{Notification, UtxosChangedNotification};
use kaspa_notify::{
    collector::{Collector, CollectorNotificationReceiver},
    error::Result,
    events::EventType,
    notification::Notification as NotificationTrait,
    notifier::DynNotify,
};
use kaspa_txindex::{PRUNING_CHUNK_SIZE, PRUNING_WAIT_INTERVAL, api::TxIndexProxy};
use kaspa_utils::triggers::SingleTrigger;
use kaspa_utxoindex::api::UtxoIndexProxy;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
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
    /// An optional TX indexer
    txindex: Option<TxIndexProxy>,

    recv_channel: CollectorNotificationReceiver<ConsensusNotification>,

    /// Has this collector been started?
    is_started: Arc<AtomicBool>,

    collect_shutdown: Arc<SingleTrigger>,
}

impl Processor {
    pub fn new(
        utxoindex: Option<UtxoIndexProxy>,
        txindex: Option<TxIndexProxy>,
        recv_channel: CollectorNotificationReceiver<ConsensusNotification>,
    ) -> Self {
        Self {
            utxoindex,
            txindex,
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
                    Ok(Some(notification)) => match notifier.notify(notification) {
                        Ok(_) => (),
                        Err(err) => {
                            trace!("[Index processor] notification sender error: {err:?}");
                        }
                    },
                    Ok(None) => (),
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

    async fn process_notification(self: &Arc<Self>, notification: ConsensusNotification) -> IndexResult<Option<Notification>> {
        match notification {
            ConsensusNotification::UtxosChanged(utxos_changed) => {
                Ok(Some(Notification::UtxosChanged(self.process_utxos_changed(utxos_changed).await?)))
            }
            ConsensusNotification::PruningPointUtxoSetOverride(pp_override) => {
                Ok(Some(Notification::PruningPointUtxoSetOverride(pp_override)))
            }
            ConsensusNotification::BlockAdded(block_added) => {
                self.process_block_added(block_added.clone()).await?;
                Ok(Some(Notification::BlockAdded(block_added)))
            }
            ConsensusNotification::VirtualChainChanged(virtual_chain_changed) => {
                self.process_virtual_chain_changed(virtual_chain_changed.clone()).await?;
                Ok(Some(Notification::VirtualChainChanged(virtual_chain_changed)))
            }
            ConsensusNotification::RetentionRootChanged(retention_root_changed) => {
                self.process_retention_root_changed(retention_root_changed.clone()).await?;
                Ok(None) // We don't expect other listeners for this notification (for now).
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

    async fn process_virtual_chain_changed(
        self: &Arc<Self>,
        notification: consensus_notification::VirtualChainChangedNotification,
    ) -> IndexResult<()> {
        trace!("[{IDENT}]: processing {:?}", notification);
        if let Some(txindex) = self.txindex.clone() {
            txindex.async_update_via_virtual_chain_changed(notification).await?;
            return Ok(());
        };
        Err(IndexError::NotSupported(EventType::VirtualChainChanged))
    }

    async fn process_block_added(self: &Arc<Self>, notification: consensus_notification::BlockAddedNotification) -> IndexResult<()> {
        trace!("[{IDENT}]: processing {:?}", notification);
        if let Some(txindex) = self.txindex.clone() {
            txindex.async_update_via_block_added(notification).await?;
            return Ok(());
        };
        Err(IndexError::NotSupported(EventType::BlockAdded))
    }

    async fn process_retention_root_changed(
        self: &Arc<Self>,
        notification: consensus_notification::RetentionRootChangedNotification,
    ) -> IndexResult<()> {
        trace!("[{IDENT}]: processing {:?}", notification);
        if let Some(txindex) = self.txindex.clone() {
            txindex.async_update_via_retention_root_changed(notification).await?;
            let self_clone = Arc::clone(self);
            // We prune in a separate task, to not block the processing of other notifications.
            info!("[Index processor] spawning on-the-fly pruning task");
            // Spawn a Tokio task that awaits the async pruning routine so it actually runs.
            tokio::spawn(async move {
                let _ = self_clone.prune_txindex().await;
            });
            return Ok(());
        };
        Err(IndexError::NotSupported(EventType::RetentionRootChanged))
    }

    async fn prune_txindex(self: &Arc<Self>) -> IndexResult<()> {
        info!("[Index processor] starting on-the-fly txindex pruning");
        if let Some(txindex) = self.txindex.clone() {
            let pruning_lock = txindex.async_get_pruning_lock().await;
            // wait on lock, in case another task is already pruning (unlikely but done for good measure).
            let _pruning_guard = pruning_lock.lock().await;
            debug!("[Index processor] acquired txindex pruning lock");
            let mut is_fully_pruned = false;
            let mut i = 0usize;
            let mut pruning_start_ts = std::time::Instant::now();
            while !is_fully_pruned && !self.collect_shutdown.trigger.is_triggered() {
                is_fully_pruned = txindex.clone().async_prune_batch().await?;
                i += 1;
                if pruning_start_ts.elapsed().as_secs() > 5 {
                    info!("[Index processor] txindex pruning - iterations: {} completed, pruned {}", i, PRUNING_CHUNK_SIZE * i as u64);
                    pruning_start_ts = std::time::Instant::now();
                }
                tokio::time::sleep(PRUNING_WAIT_INTERVAL).await; // sleep as to not be too greedy on the txindex rw lock.
            }
            info!("[Index processor] finished on-the-fly txindex pruning");
            return Ok(());
        };
        Err(IndexError::NotSupported(EventType::RetentionRootChanged))
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
    use async_channel::{Receiver, Sender, unbounded};
    use kaspa_consensus::{config::Config, consensus::test_consensus::TestConsensus, params::DEVNET_PARAMS, test_helpers::*};
    use kaspa_consensus_core::{
        acceptance_data::{AcceptedTxEntry, MergesetBlockAcceptanceData},
        tx::TransactionIndexType,
        utxo::{utxo_collection::UtxoCollection, utxo_diff::UtxoDiff},
    };
    use kaspa_consensusmanager::ConsensusManager;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_database::utils::DbLifetime;
    use kaspa_notify::notifier::test_helpers::NotifyMock;
    use kaspa_txindex::TxIndex;
    use kaspa_utxoindex::UtxoIndex;
    use rand::{SeedableRng, rngs::SmallRng};
    use std::sync::Arc;

    // TODO: rewrite with Simnet, when possible.

    #[allow(dead_code)]
    struct NotifyPipeline {
        consensus_sender: Sender<ConsensusNotification>,
        processor: Arc<Processor>,
        processor_receiver: Receiver<Notification>,
        test_consensus: TestConsensus,
        utxoindex_db_lifetime: DbLifetime,
        txindex_db_lifetime: DbLifetime,
    }

    impl NotifyPipeline {
        fn new() -> Self {
            let (consensus_sender, consensus_receiver) = unbounded();
            let (utxoindex_db_lifetime, utxoindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let (txindex_db_lifetime, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let config = Arc::new(Config::new(DEVNET_PARAMS));
            let tc = TestConsensus::new(&config);
            tc.init();
            let consensus_manager = Arc::new(ConsensusManager::from_consensus(tc.consensus_clone()));
            let utxoindex = Some(UtxoIndexProxy::new(UtxoIndex::new(consensus_manager.clone(), utxoindex_db).unwrap()));
            let txindex = Some(TxIndexProxy::new(TxIndex::new(consensus_manager, txindex_db).unwrap()));
            let processor = Arc::new(Processor::new(utxoindex, txindex, consensus_receiver));
            let (processor_sender, processor_receiver) = unbounded();
            let notifier = Arc::new(NotifyMock::new(processor_sender));
            processor.clone().start(notifier);
            Self { test_consensus: tc, consensus_sender, processor, processor_receiver, utxoindex_db_lifetime, txindex_db_lifetime }
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

    #[tokio::test]
    async fn test_block_added_notification() {
        let pipeline = NotifyPipeline::new();
        let rng = &mut SmallRng::seed_from_u64(42);
        let test_block = generate_random_block(rng, 12, 150, 10, 10);
        let test_notification = consensus_notification::BlockAddedNotification::new(test_block.clone());
        pipeline.consensus_sender.send(ConsensusNotification::BlockAdded(test_notification.clone())).await.expect("expected send");
        match pipeline.processor_receiver.recv().await.expect("expected recv") {
            Notification::BlockAdded(received_notification) => {
                assert_eq!(received_notification.block.hash(), test_block.hash());
            }
            unexpected_notification => panic!("Unexpected notification: {unexpected_notification:?}"),
        }
        assert!(pipeline.processor_receiver.is_empty(), "the notification receiver should be empty");
        pipeline.consensus_sender.close();
        pipeline.processor.clone().join().await.expect("stopping the processor must succeed");
    }

    #[tokio::test]
    async fn test_virtual_chain_changed_notification() {
        let pipeline = NotifyPipeline::new();
        let rng = &mut SmallRng::seed_from_u64(42);
        let added_chain_blocks = generate_random_hashes(rng, 5);
        let test_virtual_chain_changed_notification = consensus_notification::VirtualChainChangedNotification::new(
            Arc::new(added_chain_blocks.clone()),
            Arc::new(generate_random_hashes(rng, 3)),
            Arc::new(
                (0..5)
                    .map(|accepting_index| {
                        Arc::new(vec![MergesetBlockAcceptanceData {
                            block_hash: added_chain_blocks[accepting_index as usize],
                            accepted_transactions: (0..3)
                                .map(|tx_index| AcceptedTxEntry {
                                    transaction_id: generate_random_transaction(rng, 2, 2).id(),
                                    index_within_block: tx_index as TransactionIndexType,
                                })
                                .collect(),
                        }])
                    })
                    .collect::<Vec<_>>(),
            ),
            Arc::new([0, 1, 2, 3, 4].to_vec()),
        );
        pipeline
            .consensus_sender
            .send(ConsensusNotification::VirtualChainChanged(test_virtual_chain_changed_notification.clone()))
            .await
            .expect("expected send");
        match pipeline.processor_receiver.recv().await.expect("expected recv") {
            Notification::VirtualChainChanged(received_notification) => {
                assert_eq!(
                    received_notification.added_chain_block_hashes.as_ref(),
                    test_virtual_chain_changed_notification.added_chain_block_hashes.as_ref()
                );
                assert_eq!(
                    received_notification.removed_chain_block_hashes.as_ref(),
                    test_virtual_chain_changed_notification.removed_chain_block_hashes.as_ref()
                );
                for (i, received_acceptance_data) in received_notification.added_chain_blocks_acceptance_data.iter().enumerate() {
                    let test_acceptance_data = &test_virtual_chain_changed_notification.added_chain_blocks_acceptance_data[i];
                    assert_eq!(received_acceptance_data.len(), test_acceptance_data.len());
                    for (j, received_mbad) in received_acceptance_data.iter().enumerate() {
                        let test_mbad = &test_acceptance_data[j];
                        assert_eq!(received_mbad.block_hash, test_mbad.block_hash);
                        assert_eq!(received_mbad.accepted_transactions.len(), test_mbad.accepted_transactions.len());
                        for (k, received_ate) in received_mbad.accepted_transactions.iter().enumerate() {
                            let test_ate = &test_mbad.accepted_transactions[k];
                            assert_eq!(received_ate.transaction_id, test_ate.transaction_id);
                            assert_eq!(received_ate.index_within_block, test_ate.index_within_block);
                        }
                    }
                }
                assert_eq!(
                    received_notification.added_accepting_blue_scores.as_ref(),
                    test_virtual_chain_changed_notification.added_accepting_blue_scores.as_ref()
                );
            }
            unexpected_notification => panic!("Unexpected notification: {unexpected_notification:?}"),
        }
        assert!(pipeline.processor_receiver.is_empty(), "the notification receiver should be empty");
        pipeline.consensus_sender.close();
        pipeline.processor.clone().join().await.expect("stopping the processor must succeed");
    }

    #[tokio::test]
    async fn test_prune_txindex_executes() {
        // Ensure the on-the-fly pruning routine executes and returns without error.
        // Use a timeout so the test won't hang indefinitely if something goes wrong.
        use std::time::Duration;
        let pipeline = NotifyPipeline::new();
        let res = tokio::time::timeout(Duration::from_secs(1), pipeline.processor.prune_txindex()).await;
        assert!(res.is_ok(), "pruning timed out");
        res.unwrap().expect("pruning must succeed");
        // Cleanly shut down the processor so DB handles are released before the test ends.
        pipeline.consensus_sender.close();
        pipeline.processor.clone().join().await.expect("stopping the processor must succeed");
    }
}
