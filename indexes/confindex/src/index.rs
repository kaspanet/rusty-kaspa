use std::{cmp::min, sync::Arc};

use crate::{
    stores::{ConfIndexAcceptingBlueScoreReader, ConfIndexAcceptingBlueScoreStore, StoreManager},
    AcceptingBlueScore, AcceptingBlueScoreHashPair, ConfIndexApi, ConfIndexError, ConfIndexReindexer, ConfIndexResult, IDENT,
};
use kaspa_consensus_notify::notification::{ChainAcceptanceDataPrunedNotification, VirtualChainChangedNotification};
use kaspa_consensusmanager::{ConsensusManager, ConsensusSessionBlocking};
use kaspa_core::{debug, error, info, trace};
use kaspa_database::prelude::{StoreError, DB};
use kaspa_hashes::ZERO_HASH;
use parking_lot::RwLock;
use rocksdb::WriteBatch;

const RESYNC_CHUNKSIZE: u64 = 100_000u64; // about max 400 mbs of data under worst case (narrow dag syncing condition): (32 bytes per hash, 4 bytes per accepting blue score) * 100_000.

pub struct ConfIndex {
    stores: StoreManager,
    consensus_manager: Arc<ConsensusManager>,
}

impl ConfIndex {
    pub fn new(consensus_manager: Arc<ConsensusManager>, db: Arc<DB>) -> ConfIndexResult<Arc<RwLock<Self>>> {
        let mut confindex = Self { stores: StoreManager::new(db), consensus_manager };
        if !confindex.is_synced()? {
            match confindex.resync() {
                Ok(_) => {
                    info!("[{0}] Resync Successful", IDENT);
                }
                Err(e) => {
                    error!("[{0}] Failed to resync: {1}", IDENT, e);
                    let batch = WriteBatch::default();
                    confindex.stores.delete_all()?; // we try and delete all, in order to remove any partial data that may have been written.
                    confindex.stores.write_batch(batch)?;
                    return Err(e);
                }
            }
        }
        Ok(Arc::new(RwLock::new(confindex)))
    }

    fn update_via_reindexer(&mut self, reindexer: ConfIndexReindexer) -> ConfIndexResult<()> {
        trace!(
            "[{0}] Updating via reindexer: Range removed: {1:?}->{2:?}; Range added: {3:?}->{4:?}",
            IDENT,
            reindexer.accepting_blue_score_changes.to_remove.first(),
            reindexer.accepting_blue_score_changes.to_remove.last(),
            reindexer.accepting_blue_score_changes.to_add.first(),
            reindexer.accepting_blue_score_changes.to_add.last(),
        );
        let mut batch = WriteBatch::default();
        self.stores.accepting_blue_score_store.write_diff(&mut batch, reindexer.accepting_blue_score_changes)?;
        self.stores.write_batch(batch)?;
        Ok(())
    }

    fn sync_segement(
        &mut self,
        sync_from: AcceptingBlueScoreHashPair,
        sync_to: AcceptingBlueScoreHashPair,
        remove_segment: bool,
        session: &ConsensusSessionBlocking<'_>,
    ) -> ConfIndexResult<()> {
        debug!("sync_segement: sync_from: {:?}, sync_to: {:?}, remove_segment: {:?}", sync_from, sync_to, remove_segment);
        let total_blue_score_to_process = sync_to.accepting_blue_score - sync_from.accepting_blue_score;
        let mut current =
            AcceptingBlueScoreHashPair::new(sync_from.accepting_blue_score, if remove_segment { ZERO_HASH } else { sync_from.hash });
        info!("[{0}] {1}: {2} Blue Scores", IDENT, if remove_segment { "Unsyncing" } else { "Syncing" }, total_blue_score_to_process);
        let mut total_blue_score_processed = (0u64, 0u64); // .0 holds the value of the former display
        let mut percent_completed = (0f64, 0f64); // .0 holds the value of the former display
        let percent_display_granularity = 1.0; // in percent
        let mut instant = std::time::Instant::now();
        let mut is_end = false;
        let mut is_start = true;

        while !is_end {
            let mut batch = WriteBatch::default();
            if remove_segment {
                self.stores.accepting_blue_score_store.remove_many(
                    &mut batch,
                    (current.accepting_blue_score..=(current.accepting_blue_score + RESYNC_CHUNKSIZE)).collect(),
                )?;
                current = AcceptingBlueScoreHashPair::new(
                    min(current.accepting_blue_score + RESYNC_CHUNKSIZE, sync_to.accepting_blue_score),
                    ZERO_HASH,
                );
            } else {
                let mut to_add = session
                    .get_virtual_chain_from_block(current.hash, None, RESYNC_CHUNKSIZE.try_into().unwrap())?
                    .added
                    .into_iter()
                    .take(RESYNC_CHUNKSIZE.try_into().unwrap())
                    .map(|h| AcceptingBlueScoreHashPair::new(session.get_header(h).unwrap().blue_score, h))
                    .collect::<Vec<_>>();
                if is_start {
                    //get virtual chain is none-inclusive in respect to low, so we prepend the current.
                    to_add.insert(0, current.clone());
                }
                if let Some(last) = to_add.last() {
                    current = last.clone();
                }
                self.stores.accepting_blue_score_store.write_many(&mut batch, to_add)?;
            }
            total_blue_score_processed.1 = current.accepting_blue_score - sync_from.accepting_blue_score;
            percent_completed.1 = total_blue_score_processed.1 as f64 / total_blue_score_to_process as f64 * 100.0;
            is_end = current.accepting_blue_score >= sync_to.accepting_blue_score;
            if is_start {
                total_blue_score_processed.0 = sync_from.accepting_blue_score;
                is_start = false
            };
            self.stores.write_batch(batch)?;
            if percent_completed.0 + percent_display_granularity <= percent_completed.1 || is_end {
                let total_blue_score_processed_diff = total_blue_score_processed.1 - total_blue_score_processed.0;

                info!(
                    "[{0}] {1} - Blue score: {2}+{3}/{4}  ({5:.0}/s); {6:.0}%",
                    IDENT,
                    if remove_segment { "Removed" } else { "Added" },
                    total_blue_score_processed.1,
                    total_blue_score_processed_diff,
                    total_blue_score_to_process,
                    total_blue_score_processed_diff as f64 / instant.elapsed().as_secs_f64(),
                    if is_end { 100.0 } else { percent_completed.1 },
                );

                percent_completed.0 = percent_completed.1;
                total_blue_score_processed.0 = total_blue_score_processed.1;
                instant = std::time::Instant::now();
            }
        }
        Ok(())
    }
}

impl ConfIndexApi for ConfIndex {
    fn resync(&mut self) -> ConfIndexResult<()> {
        trace!("[{0}] Started Resyncing", IDENT);
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        // Gather the necessary potential block hashes to sync from and to.
        let confindex_source_blue_score_pair = match self.stores.accepting_blue_score_store.get_source() {
            Ok(confindex_source) => Some(confindex_source),
            Err(StoreError::DbEmptyError) => None,
            Err(err) => return Err(ConfIndexError::from(err)),
        };
        debug!("confindex_source_blue_score_pair: {:?}", confindex_source_blue_score_pair);
        let confindex_sink_blue_score_pair = match self.stores.accepting_blue_score_store.get_sink() {
            Ok(confindex_sink) => Some(confindex_sink),
            Err(StoreError::DbEmptyError) => None,
            Err(err) => return Err(ConfIndexError::from(err)),
        };
        debug!("confindex_sink_blue_score_pair: {:?}", confindex_sink_blue_score_pair);
        let consensus_source_blue_score_pair = {
            let hash = session.get_source(true);
            let blue_score = session.get_header(hash)?.blue_score;
            AcceptingBlueScoreHashPair::new(blue_score, hash)
        };
        debug!("consensus_source_blue_score_pair: {:?}", consensus_source_blue_score_pair);
        let consensus_sink_blue_score_pair = {
            let hash = session.get_sink();
            let blue_score = session.get_header(hash)?.blue_score;
            AcceptingBlueScoreHashPair::new(blue_score, hash)
        };
        debug!("consensus_sink_blue_score_pair: {:?}", consensus_sink_blue_score_pair);

        let split_point_blue_score_pair = if let Some(confindex_sink_blue_score_pair) = confindex_sink_blue_score_pair.clone() {
            if confindex_sink_blue_score_pair == consensus_sink_blue_score_pair {
                None // no need to resync along DAG end
            } else {
                let hash = session
                    .find_highest_common_chain_block(confindex_sink_blue_score_pair.hash, consensus_sink_blue_score_pair.hash)?;
                Some(AcceptingBlueScoreHashPair::new(session.get_header(hash)?.blue_score, hash))
            }
        } else {
            None
        };

        // Sanity checks
        if confindex_source_blue_score_pair.is_none() {
            assert!(confindex_sink_blue_score_pair.is_none()); // db shouldn't allow source to be None, and sink to be Some.
        };
        if confindex_sink_blue_score_pair.is_none() {
            assert!(confindex_source_blue_score_pair.is_none()); // db shouldn't allow sink to be None, and source to be Some.
        };
        if confindex_sink_blue_score_pair.is_some() {
            assert!(confindex_source_blue_score_pair.is_some()); // db shouldn't allow sink to be Some, and source to be None.
        };
        if confindex_source_blue_score_pair.is_some() {
            assert!(confindex_sink_blue_score_pair.is_some()); // db shouldn't allow source to be Some, and sink to be None.
        };

        // Determine the resync points
        let resync_points = if let Some(confindex_sink_blue_score_pair) = confindex_sink_blue_score_pair.clone() {
            if confindex_sink_blue_score_pair == consensus_sink_blue_score_pair {
                None // no need to resync along DAG end
            } else {
                Some((split_point_blue_score_pair.clone().unwrap(), consensus_sink_blue_score_pair))
            }
        } else {
            Some((consensus_source_blue_score_pair.clone(), consensus_sink_blue_score_pair))
        };

        let mut unsync_points = Vec::new();
        unsync_points.push(
            split_point_blue_score_pair
                .clone()
                .map(|split_point_blue_score_pair| (split_point_blue_score_pair, confindex_sink_blue_score_pair.unwrap().clone())),
        );

        unsync_points.push({
            if let Some(confindex_source_blue_score_pair) = confindex_source_blue_score_pair {
                // Sanity check
                assert!(
                    confindex_source_blue_score_pair.accepting_blue_score <= consensus_source_blue_score_pair.accepting_blue_score
                );
                if confindex_source_blue_score_pair.accepting_blue_score < consensus_source_blue_score_pair.accepting_blue_score {
                    Some((confindex_source_blue_score_pair, consensus_source_blue_score_pair))
                } else {
                    None
                }
            } else {
                None
            }
        });

        // unsync the segments
        for (from, to) in unsync_points.into_iter().flatten() {
            self.sync_segement(from, to, true, &session)?;
        }

        // resync the segments
        if let Some((from, to)) = resync_points {
            self.sync_segement(from, to, false, &session)?;
        };

        Ok(())
    }

    fn is_synced(&self) -> ConfIndexResult<bool> {
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        let confindex_source = match self.stores.accepting_blue_score_store.get_source() {
            Ok(confindex_source) => confindex_source,
            Err(StoreError::DbEmptyError) => return Ok(false),
            Err(err) => return Err(ConfIndexError::from(err)),
        };
        let confindex_sink = match self.stores.accepting_blue_score_store.get_sink() {
            Ok(confindex_source) => confindex_source,
            Err(StoreError::DbEmptyError) => return Ok(false),
            Err(err) => return Err(ConfIndexError::from(err)),
        };

        if confindex_source.hash == session.get_source(true) && confindex_sink.hash == session.get_sink() {
            return Ok(true);
        }

        Ok(false)
    }

    fn get_accepting_blue_score_chain_blocks(
        &self,
        from: AcceptingBlueScore,
        to: AcceptingBlueScore,
    ) -> ConfIndexResult<Arc<Vec<AcceptingBlueScoreHashPair>>> {
        trace!("[{0}] Getting accepting blue score chain blocks along {1} => {2}", IDENT, from, to);
        Ok(Arc::new(self.stores.accepting_blue_score_store.get_range(from, to)?))
    }

    fn get_sink(&self) -> ConfIndexResult<AcceptingBlueScoreHashPair> {
        trace!("[{0}] Getting sink", IDENT);
        Ok(self.stores.accepting_blue_score_store.get_sink()?)
    }

    fn get_source(&self) -> ConfIndexResult<AcceptingBlueScoreHashPair> {
        trace!("[{0}] Getting source", IDENT);
        Ok(self.stores.accepting_blue_score_store.get_source()?)
    }

    fn update_via_virtual_chain_changed(
        &mut self,
        virtual_chain_changed_notification: VirtualChainChangedNotification,
    ) -> ConfIndexResult<()> {
        trace!(
            "[{0}] Updating via virtual chain changed notification: {1} added, {2} removed, {3:?}",
            IDENT,
            virtual_chain_changed_notification.added_chain_block_hashes.len(),
            virtual_chain_changed_notification.removed_chain_block_hashes.len(),
            virtual_chain_changed_notification
        );
        self.update_via_reindexer(ConfIndexReindexer::from(virtual_chain_changed_notification))
    }

    fn update_via_chain_acceptance_data_pruned(
        &mut self,
        chain_acceptance_data_pruned_notification: ChainAcceptanceDataPrunedNotification,
    ) -> ConfIndexResult<()> {
        trace!(
            "[{0}] Updating via chain acceptance data pruned notification: Blue score: {1} removed",
            IDENT,
            chain_acceptance_data_pruned_notification.mergeset_block_acceptance_data_pruned.accepting_blue_score
        );
        self.update_via_reindexer(ConfIndexReindexer::from(chain_acceptance_data_pruned_notification))
    }

    fn get_all_hash_blue_score_pairs(&self) -> ConfIndexResult<Arc<Vec<AcceptingBlueScoreHashPair>>> {
        trace!("[{0}] Getting all hash blue score pairs", IDENT);
        Ok(Arc::new(self.stores.accepting_blue_score_store.get_all()?))
    }
}

impl std::fmt::Debug for ConfIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfIndex").finish()
    }
}

#[cfg(test)]
pub mod test {
    use std::sync::Arc;

    use kaspa_consensus::{
        consensus::test_consensus::{TestConsensus, TestConsensusFactory},
        params::MAINNET_PARAMS,
    };
    use kaspa_consensus_core::{acceptance_data::AcceptanceData, config::Config};
    use kaspa_consensus_notify::notification::{ChainAcceptanceDataPrunedNotification, VirtualChainChangedNotification};
    use kaspa_consensusmanager::ConsensusManager;
    use kaspa_core::warn;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    use kaspa_hashes::Hash;

    use crate::{AcceptingBlueScoreHashPair, ConfIndex, ConfIndexApi};

    #[test]
    fn test_confindex_update() {
        kaspa_core::log::try_init_logger("TRACE");

        // Set-up:
        let (_confindex_db_lt, confindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (_tc_db_lt, tc_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        let tc_config = Config::new(MAINNET_PARAMS);

        let tc = Arc::new(TestConsensus::with_db(tc_db.clone(), &tc_config));
        let tcm = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(tc.clone()))));
        let confindex = ConfIndex::new(tcm, confindex_db).unwrap();

        // Define the block hashes:

        let block_a_pair = AcceptingBlueScoreHashPair { accepting_blue_score: 0, hash: Hash::from_u64_word(1) };
        let block_b_pair = AcceptingBlueScoreHashPair { accepting_blue_score: 1, hash: Hash::from_u64_word(2) };
        let block_c_pair = AcceptingBlueScoreHashPair { accepting_blue_score: 2, hash: Hash::from_u64_word(3) };
        let block_d_pair = AcceptingBlueScoreHashPair { accepting_blue_score: 2, hash: Hash::from_u64_word(4) };

        // add blocks a, b, c, to the confindex in one notification
        let update_1 = VirtualChainChangedNotification {
            added_chain_block_hashes: vec![block_a_pair.hash, block_b_pair.hash, block_c_pair.hash].into(),
            added_chain_blocks_acceptance_data: Arc::new(vec![
                Arc::new(AcceptanceData {
                    accepting_blue_score: block_a_pair.accepting_blue_score,
                    mergeset: vec![], // irrelevant
                }),
                Arc::new(AcceptanceData {
                    accepting_blue_score: block_b_pair.accepting_blue_score,
                    mergeset: vec![], // irrelevant
                }),
                Arc::new(AcceptanceData {
                    accepting_blue_score: block_c_pair.accepting_blue_score,
                    mergeset: vec![], // irrelevant
                }),
            ]),
            removed_chain_block_hashes: vec![].into(),
            removed_chain_blocks_acceptance_data: vec![].into(),
        };

        confindex.write().update_via_virtual_chain_changed(update_1).unwrap();
        assert_eq!(confindex.read().get_source().unwrap(), block_a_pair.clone());
        assert_eq!(confindex.read().get_sink().unwrap(), block_c_pair.clone());
        assert_eq!(
            confindex.read().get_accepting_blue_score_chain_blocks(0, 3).unwrap(),
            vec![block_a_pair.clone(), block_b_pair.clone(), block_c_pair.clone()].into()
        );
        assert_eq!(confindex.read().get_all_hash_blue_score_pairs().unwrap().len(), 3);

        // reorg block c from the confindex with block d in one notification
        let update_2 = VirtualChainChangedNotification {
            added_chain_block_hashes: vec![block_d_pair.clone().hash].into(),
            added_chain_blocks_acceptance_data: Arc::new(vec![Arc::new(AcceptanceData {
                accepting_blue_score: block_d_pair.accepting_blue_score,
                mergeset: vec![], // irrelevant
            })]),
            removed_chain_block_hashes: vec![block_c_pair.clone().hash].into(),
            removed_chain_blocks_acceptance_data: Arc::new(vec![Arc::new(AcceptanceData {
                accepting_blue_score: block_c_pair.accepting_blue_score,
                mergeset: vec![], // irrelevant
            })]),
        };

        confindex.write().update_via_virtual_chain_changed(update_2).unwrap();
        assert_eq!(confindex.read().get_source().unwrap(), block_a_pair.clone());
        assert_eq!(confindex.read().get_sink().unwrap(), block_d_pair.clone());
        assert_eq!(
            confindex.read().get_accepting_blue_score_chain_blocks(0, 2).unwrap(),
            vec![block_a_pair.clone(), block_b_pair.clone(), block_d_pair.clone()].into()
        );
        assert_eq!(confindex.read().get_all_hash_blue_score_pairs().unwrap().len(), 3);

        // prune block a from the confindex in one notification
        let update_3 = ChainAcceptanceDataPrunedNotification {
            mergeset_block_acceptance_data_pruned: Arc::new(AcceptanceData {
                accepting_blue_score: block_a_pair.accepting_blue_score,
                mergeset: vec![], // irrelevant
            }),
            chain_hash_pruned: block_a_pair.hash,
            source: block_b_pair.hash,
        };

        confindex.write().update_via_chain_acceptance_data_pruned(update_3).unwrap();
        assert_eq!(confindex.read().get_source().unwrap(), block_b_pair);
        assert_eq!(confindex.read().get_sink().unwrap(), block_d_pair);
        assert_eq!(confindex.read().get_accepting_blue_score_chain_blocks(1, 2).unwrap(), vec![block_b_pair, block_d_pair].into());
        assert_eq!(confindex.read().get_all_hash_blue_score_pairs().unwrap().len(), 2);
    }

    #[test]
    fn test_confindex_resync() {
        kaspa_core::log::try_init_logger("WARN");
        warn!("Test is not implemented yet..");
        //TODO: implement test - ideally move to, and expand on simpa.
        //see: https://github.com/kaspanet/rusty-kaspa/issues/59
    }
}
