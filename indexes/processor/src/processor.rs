use crate::{
    errors::{IndexError, IndexResult},
    IDENT,
};
use async_trait::async_trait;
use kaspa_consensus_notify::{notification as consensus_notification, notification::Notification as ConsensusNotification};
use kaspa_core::{debug, trace, warn};
use kaspa_index_core::notify::{notification as index_notification, notification::Notification as IndexNotification};
use kaspa_notify::{
    collector::{Collector, CollectorNotificationReceiver},
    error::Result,
    events::EventType,
    notification::Notification,
    notifier::DynNotify,
};
use kaspa_txindex::api::TxIndexProxy;
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

    fn spawn_collecting_task(self: Arc<Self>, notifier: DynNotify<IndexNotification>) {
        // The task can only be spawned once
        if self.is_started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        tokio::spawn(async move {
            trace!("[Index processor] collecting task starting");

            while let Ok(notification) = self.recv_channel.recv().await {
                match self.process_notification(notification).await {
                    Ok(notification) => match notification {
                        Some(notification) => match notifier.notify(notification.clone()) {
                            Ok(_) => trace!("[Index processor] sent notification: {notification:?}"),
                            Err(err) => warn!("[Index processor] notification sender error: {err:?}"),
                        },
                        None => trace!("[Index processor] notification was filtered out"),
                    },
                    Err(err) => {
                        warn!("[Index processor] error while processing a consensus notification: {err:?}");
                    }
                }
            }

            debug!("[Index processor] notification stream ended");
            self.collect_shutdown.trigger.trigger();
            trace!("[Index processor] collecting task ended");
        });
    }

    async fn process_notification(
        self: &Arc<Self>,
        notification: consensus_notification::Notification,
    ) -> IndexResult<Option<IndexNotification>> {
        trace!("[{IDENT}]: processing {:?}", notification);
        match notification {
            ConsensusNotification::UtxosChanged(utxos_changed_notification) => {
                let utxos_changed_notification = self.process_utxos_changed(utxos_changed_notification).await?; // Converts to `kaspa_index_core::notification::Notification` here
                Ok(Some(IndexNotification::UtxosChanged(utxos_changed_notification)))
            }
            ConsensusNotification::PruningPointUtxoSetOverride(prunging_point_utxo_set_override_notification) => {
                // Convert to `kaspa_index_core::notification::Notification`
                Ok(Some(IndexNotification::PruningPointUtxoSetOverride(prunging_point_utxo_set_override_notification.into())))
            }
            ConsensusNotification::VirtualChainChanged(virtual_chain_chainged_notification) => {
                Ok(Some(IndexNotification::VirtualChainChanged(
                    self.process_virtual_chain_changed_notification(virtual_chain_chainged_notification).await?,
                )))
            }
            ConsensusNotification::ChainAcceptanceDataPruned(chain_acceptance_data_pruned) => {
                self.process_chain_acceptance_data_pruned(chain_acceptance_data_pruned).await?;
                Ok(None)
            }
            _ => Err(IndexError::NotSupported(notification.event_type())),
        }
    }

    async fn process_utxos_changed(
        self: &Arc<Self>,
        notification: consensus_notification::UtxosChangedNotification,
    ) -> IndexResult<index_notification::UtxosChangedNotification> {
        if let Some(utxoindex) = self.utxoindex.clone() {
            let converted_notification: index_notification::UtxosChangedNotification = utxoindex.update(notification).await?.into();
            debug!(
                "[Index processor] Creating UtxosChanged notifications with {0} added and {1} removed utxos",
                converted_notification.added.len(),
                converted_notification.removed.len()
            );
            return Ok(converted_notification);
        };
        Err(IndexError::NotSupported(EventType::UtxosChanged))
    }

    async fn process_virtual_chain_changed_notification(
        self: &Arc<Self>,
        notification: consensus_notification::VirtualChainChangedNotification,
    ) -> IndexResult<index_notification::VirtualChainChangedNotification> {
        if let Some(txindex) = self.txindex.clone() {
            txindex.update_via_virtual_chain_changed(notification.clone()).await?;
            debug!(
                "[Index processor] updated txindex with {0} added and {1} removed chain blocks",
                notification.added_chain_block_hashes.len(),
                notification.removed_chain_block_hashes.len()
            );
            return Ok(notification.into());
        };
        Err(IndexError::NotSupported(EventType::VirtualChainChanged))
    }

    async fn process_chain_acceptance_data_pruned(
        self: &Arc<Self>,
        notification: consensus_notification::ChainAcceptanceDataPrunedNotification,
    ) -> IndexResult<()> {
        if let Some(txindex) = self.txindex.clone() {
            txindex.update_via_chain_acceptance_data_pruned(notification.clone()).await?;
            debug!(
                "[Index processor] updated txindex with {0} pruned chain blocks",
                notification.mergeset_block_acceptance_data_pruned.len(),
            );
            return Ok(());
        };
        Err(IndexError::NotSupported(EventType::ChainAcceptanceDataPruned))
    }

    async fn join_collecting_task(&self) -> Result<()> {
        trace!("[Index processor] joining");
        self.collect_shutdown.listener.clone().await;
        debug!("[Index processor] terminated");
        Ok(())
    }
}

#[async_trait]
impl Collector<IndexNotification> for Processor {
    fn start(self: Arc<Self>, notifier: DynNotify<IndexNotification>) {
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
    use kaspa_consensus::{
        config::Config as ConsensusConfig,
        consensus::test_consensus::TestConsensus,
        params::DEVNET_PARAMS,
        testutils::generate::from_rand::{
            acceptance_data::{generate_random_acceptance_data, generate_random_acceptance_data_vec},
            hash::{generate_random_hash, generate_random_hashes},
            utxo::{generate_random_outpoint, generate_random_utxo},
        },
    };
    use kaspa_consensus_core::utxo::{utxo_collection::UtxoCollection, utxo_diff::UtxoDiff};
    use kaspa_consensusmanager::ConsensusManager;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_database::utils::DbLifetime;
    use kaspa_notify::notifier::test_helpers::NotifyMock;
    use kaspa_txindex::{config::Config as TxIndexConfig, TxIndex};
    use kaspa_utxoindex::UtxoIndex;
    use rand::{rngs::SmallRng, SeedableRng};
    use std::sync::Arc;

    // TODO: rewrite with Simnet, when possible.

    #[allow(dead_code)]
    struct NotifyPipeline {
        consensus_sender: Sender<ConsensusNotification>,
        pub processor: Arc<Processor>,
        processor_receiver: Receiver<IndexNotification>,
        test_consensus: TestConsensus,
        utxoindex_db_lifetime: DbLifetime,
        txindex_db_lifetime: DbLifetime,
    }

    impl NotifyPipeline {
        fn new() -> Self {
            let (consensus_sender, consensus_receiver) = unbounded();
            let (utxoindex_db_lifetime, utxoindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let (txindex_db_lifetime, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
            let consensus_config = Arc::new(ConsensusConfig::new(DEVNET_PARAMS));
            let tc = TestConsensus::new(&consensus_config);
            tc.init();
            let consensus_manager = Arc::new(ConsensusManager::from_consensus(tc.consensus_clone()));
            let utxoindex = Some(UtxoIndexProxy::new(UtxoIndex::new(consensus_manager.clone(), utxoindex_db).unwrap()));
            let txindex_config = Arc::new(TxIndexConfig::from(&consensus_config));
            let txindex = Some(TxIndexProxy::new(TxIndex::new(consensus_manager, txindex_db, txindex_config).unwrap()));
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
            IndexNotification::UtxosChanged(utxo_changed_notification) => {
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
                        // Assert data is added to the db:
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
            IndexNotification::PruningPointUtxoSetOverride(_) => (),
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

        let to_add_acceptance_data = generate_random_acceptance_data_vec(rng, 42, 18, 200, 1.0 / 3.0);
        let to_add_hashes = generate_random_hashes(rng, to_add_acceptance_data.len() - 1);
        let to_remove_acceptance_data = generate_random_acceptance_data_vec(rng, 42, 18, 200, 2.0 / 3.0);
        let to_remove_hashes = generate_random_hashes(rng, to_remove_acceptance_data.len() - 1);

        let test_notification = consensus_notification::VirtualChainChangedNotification::new(
            Arc::new(to_add_hashes.clone()),
            Arc::new(to_remove_hashes.clone()),
            Arc::new(to_add_acceptance_data.clone()),
            Arc::new(to_remove_acceptance_data.clone()),
        );

        pipeline
            .consensus_sender
            .send(ConsensusNotification::VirtualChainChanged(test_notification.clone()))
            .await
            .expect("expected send");

        match pipeline.processor_receiver.recv().await.expect("receives a notification") {
            IndexNotification::VirtualChainChanged(virtual_chain_changed_notification) => {
                // Assert length of added and removed match
                assert_eq!(
                    test_notification.added_chain_block_hashes.len(),
                    virtual_chain_changed_notification.added_chain_block_hashes.len()
                );
                assert_eq!(
                    test_notification.removed_chain_block_hashes.len(),
                    virtual_chain_changed_notification.removed_chain_block_hashes.len()
                );
                assert_eq!(
                    test_notification.added_chain_blocks_acceptance_data.len(),
                    virtual_chain_changed_notification.added_chain_blocks_acceptance_data.len()
                );
                assert_eq!(
                    test_notification.removed_chain_blocks_acceptance_data.len(),
                    virtual_chain_changed_notification.removed_chain_blocks_acceptance_data.len()
                );
                // Assert added hashes match
                for (test_hash, notification_hash) in test_notification
                    .added_chain_block_hashes
                    .iter()
                    .zip(virtual_chain_changed_notification.added_chain_block_hashes.iter())
                {
                    assert_eq!(test_hash, notification_hash);
                }
                // Assert removed hashes match
                for (test_hash, notification_hash) in test_notification
                    .removed_chain_block_hashes
                    .iter()
                    .zip(virtual_chain_changed_notification.removed_chain_block_hashes.iter())
                {
                    assert_eq!(test_hash, notification_hash);
                }
                // Assert added acceptance data match
                for (test_mergesets, notification_mergesets) in test_notification
                    .added_chain_blocks_acceptance_data
                    .iter()
                    .zip(virtual_chain_changed_notification.added_chain_blocks_acceptance_data.iter())
                {
                    assert_eq!(test_mergesets.len(), notification_mergesets.len());
                    for (test_mergeset, notification_mergeset) in test_mergesets.iter().zip(notification_mergesets.iter()) {
                        assert_eq!(test_mergeset.block_hash, notification_mergeset.block_hash);
                        assert_eq!(test_mergeset.accepted_transactions.len(), notification_mergeset.accepted_transactions.len());
                        for (test_tx_entry, notification_tx_entry) in
                            test_mergeset.accepted_transactions.iter().zip(notification_mergeset.accepted_transactions.iter())
                        {
                            assert_eq!(test_tx_entry.transaction_id, notification_tx_entry.transaction_id);
                            assert_eq!(test_tx_entry.index_within_block, notification_tx_entry.index_within_block);
                        }
                    }
                }
                // Assert removed acceptance data match
                for (test_mergesets, notification_mergesets) in test_notification
                    .removed_chain_blocks_acceptance_data
                    .iter()
                    .zip(virtual_chain_changed_notification.removed_chain_blocks_acceptance_data.iter())
                {
                    assert_eq!(test_mergesets.len(), notification_mergesets.len());
                    for (test_mergeset, notification_mergeset) in test_mergesets.iter().zip(notification_mergesets.iter()) {
                        assert_eq!(test_mergeset.block_hash, notification_mergeset.block_hash);
                        assert_eq!(test_mergeset.accepted_transactions.len(), notification_mergeset.accepted_transactions.len());
                        for (test_tx_entry, notification_tx_entry) in
                            test_mergeset.accepted_transactions.iter().zip(notification_mergeset.accepted_transactions.iter())
                        {
                            assert_eq!(test_tx_entry.transaction_id, notification_tx_entry.transaction_id);
                            assert_eq!(test_tx_entry.index_within_block, notification_tx_entry.index_within_block);
                        }
                    }
                }
            }
            unexpected_notification => panic!("Unexpected notification: {unexpected_notification:?}"),
        }
        assert!(pipeline.processor_receiver.is_empty(), "the notification receiver should be empty");
        pipeline.consensus_sender.close();
        pipeline.processor.clone().join().await.expect("stopping the processor must succeed");
    }

    #[tokio::test]
    async fn test_chain_acceptance_data_pruned_notification() {
        let pipeline = NotifyPipeline::new();
        let rng = &mut SmallRng::seed_from_u64(42);

        let chain_hash_pruned = generate_random_hash(rng);
        let mergeset_block_acceptance_data_pruned = generate_random_acceptance_data(rng, 18, 200, 1.0 / 3.0);
        let source = generate_random_hash(rng);

        let test_notification = consensus_notification::ChainAcceptanceDataPrunedNotification::new(
            chain_hash_pruned,
            Arc::new(mergeset_block_acceptance_data_pruned.clone()),
            source,
        );

        pipeline
            .consensus_sender
            .send(ConsensusNotification::ChainAcceptanceDataPruned(test_notification.clone()))
            .await
            .expect("expected send");

        // we expect no index notification response to be sent, so below is enough for the test
        assert!(pipeline.processor_receiver.is_empty(), "the notification receiver should be empty");
        // TODO: We can none-the-less check that the notification was processed correctly via the txindex itself
        pipeline.consensus_sender.close();
        pipeline.processor.clone().join().await.expect("stopping the processor must succeed");
    }
}
