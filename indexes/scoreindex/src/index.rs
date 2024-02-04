use std::{cmp::min, sync::Arc};

use crate::{
    stores::{ScoreIndexAcceptingBlueScoreReader, ScoreIndexAcceptingBlueScoreStore, StoreManager},
    AcceptingBlueScore, AcceptingBlueScoreHashPair, ScoreIndexResult, IDENT,
};
use kaspa_consensus_notify::notification::VirtualChainChangedNotification;
use kaspa_consensusmanager::{ConsensusManager, ConsensusSessionBlocking};
use kaspa_core::{error, info, trace};
use kaspa_database::prelude::DB;
use kaspa_hashes::ZERO_HASH;
use parking_lot::RwLock;
use rocksdb::WriteBatch;

const RESYNC_CHUNKSIZE: u64 = 100_000u64; // about max 400 mbs of data under worst case (narrow dag): (32 bytes per hash, 4 bytes per accepting blue score) * 100_000.

pub struct ScoreIndex {
    stores: StoreManager,
    consensus_manager: Arc<ConsensusManager>,
}

impl ScoreIndex {
    pub fn new(consensus_manager: Arc<ConsensusManager>, db: Arc<DB>) -> ScoreIndexResult<Arc<RwLock<Self>>> {
        let mut scoreindex = Self { stores: StoreManager::new(db), consensus_manager };
        if !scoreindex.is_synced()? {
            match scoreindex.resync() {
                Ok(_) => {
                    info!("[{0}] Resync Successful", IDENT);
                }
                Err(e) => {
                    error!("[{0}] Failed to resync: {1}", IDENT, e);
                    let batch = WriteBatch::default();
                    scoreindex.stores.delete_all()?; // we try and delete all, in order to remove any partial data that may have been written.
                    scoreindex.stores.write_batch(batch)?;
                    return Err(e);
                }
            }
        }
        Ok(Arc::new(RwLock::new(scoreindex)))
    }

    fn sync_segement(
        &mut self,
        sync_from: AcceptingBlueScoreHashPair,
        sync_to: AcceptingBlueScoreHashPair,
        remove_segment: bool,
        session: &ConsensusSessionBlocking<'_>,
    ) -> ScoreIndexResult<()> {
        let total_blue_score_to_process = sync_from.accepting_blue_score - sync_to.accepting_blue_score;
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
                    min(current.accepting_blue_score + RESYNC_CHUNKSIZE, total_blue_score_to_process),
                    ZERO_HASH,
                );
            } else {
                let to_add = session
                    .get_virtual_chain_from_block(current.hash, None, RESYNC_CHUNKSIZE.try_into().unwrap())?
                    .added
                    .into_iter()
                    .take(RESYNC_CHUNKSIZE.try_into().unwrap())
                    .map(|h| AcceptingBlueScoreHashPair::new(session.get_header(h).unwrap().blue_score, h))
                    .collect::<Vec<_>>();
                current = to_add.last().unwrap().clone();
                self.stores.accepting_blue_score_store.write_many(&mut batch, to_add)?;
            }
            total_blue_score_processed.1 = current.accepting_blue_score;
            percent_completed.1 = total_blue_score_processed.1 as f64 / total_blue_score_to_process as f64 * 100.0;
            is_end = current.accepting_blue_score >= sync_to.accepting_blue_score;
            if is_start {
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

impl ScoreIndex {
    pub fn resync(&mut self) -> ScoreIndexResult<()> {
        trace!("[{0}] Started Resyncing", IDENT);
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        // Gather the necessary potential block hashes to sync from and to.
        let scoreindex_source_blue_score_pair = self.stores.accepting_blue_score_store.get_source()?;
        let scoreindex_sink_blue_score_pair = self.stores.accepting_blue_score_store.get_sink()?;
        let consensus_source_blue_score_pair = {
            let hash = session.get_source();
            let blue_score = session.get_header(hash)?.blue_score;
            AcceptingBlueScoreHashPair::new(blue_score, hash)
        };
        let consensus_sink_blue_score_pair = {
            let hash = session.get_sink();
            let blue_score = session.get_header(hash)?.blue_score;
            AcceptingBlueScoreHashPair::new(blue_score, hash)
        };

        let split_point_blue_score_pair = if let Some(scoreindex_sink_blue_score_pair) = scoreindex_sink_blue_score_pair.clone() {
            if scoreindex_sink_blue_score_pair == consensus_sink_blue_score_pair {
                None // no need to resync along DAG end
            } else {
                let hash = session
                    .find_highest_common_chain_block(scoreindex_sink_blue_score_pair.hash, consensus_sink_blue_score_pair.hash)?;
                Some(AcceptingBlueScoreHashPair::new(session.get_header(hash)?.blue_score, hash))
            }
        } else {
            None
        };

        // Sanity checks
        if scoreindex_source_blue_score_pair.is_none() {
            assert!(scoreindex_sink_blue_score_pair.is_none()); // db shouldn't allow source to be None, and sink to be Some.
        };
        if scoreindex_sink_blue_score_pair.is_none() {
            assert!(scoreindex_source_blue_score_pair.is_none()); // db shouldn't allow sink to be None, and source to be Some.
        };
        if scoreindex_sink_blue_score_pair.is_some() {
            assert!(scoreindex_source_blue_score_pair.is_some()); // db shouldn't allow sink to be Some, and source to be None.
        };
        if scoreindex_source_blue_score_pair.is_some() {
            assert!(scoreindex_sink_blue_score_pair.is_some()); // db shouldn't allow source to be Some, and sink to be None.
        };

        // Determine the resync points
        let resync_points = if let Some(scoreindex_sink_blue_score_pair) = scoreindex_sink_blue_score_pair.clone() {
            if scoreindex_sink_blue_score_pair == consensus_sink_blue_score_pair {
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
                .map(|split_point_blue_score_pair| (split_point_blue_score_pair, scoreindex_sink_blue_score_pair.unwrap().clone())),
        );

        unsync_points.push({
            if let Some(scoreindex_source_blue_score_pair) = scoreindex_source_blue_score_pair {
                // Sanity check
                assert!(
                    scoreindex_source_blue_score_pair.accepting_blue_score <= consensus_source_blue_score_pair.accepting_blue_score
                );
                if scoreindex_source_blue_score_pair.accepting_blue_score < consensus_source_blue_score_pair.accepting_blue_score {
                    Some((scoreindex_source_blue_score_pair, consensus_source_blue_score_pair))
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

    pub fn is_synced(&self) -> ScoreIndexResult<bool> {
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());
        if let Some(scoreindex_sink) = self.stores.accepting_blue_score_store.get_sink()? {
            if scoreindex_sink.hash == session.get_sink() {
                if let Some(scoreindex_source) = self.stores.accepting_blue_score_store.get_source()? {
                    if scoreindex_source.hash == session.get_source() {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    pub fn get_accepting_blue_score_chain_blocks(
        &self,
        _from: AcceptingBlueScore,
        _to: AcceptingBlueScore,
    ) -> ScoreIndexResult<Arc<Vec<AcceptingBlueScoreHashPair>>> {
        todo!()
    }

    pub fn get_sink(&self) -> ScoreIndexResult<Option<AcceptingBlueScoreHashPair>> {
        todo!()
    }

    pub fn get_source(&self) -> ScoreIndexResult<Option<AcceptingBlueScoreHashPair>> {
        todo!()
    }

    pub fn update_via_virtual_chain_changed(
        &self,
        _virtual_chain_changed_notification: VirtualChainChangedNotification,
    ) -> ScoreIndexResult<()> {
        todo!()
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn test_score_index() {
        todo!()
    }
}
