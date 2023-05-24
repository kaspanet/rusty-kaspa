//! TODO: module comment about locking safety and consistency of various pruning stores

use crate::{
    consensus::{
        services::{ConsensusServices, DbGhostdagManager, DbPruningPointManager},
        storage::ConsensusStorage,
    },
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            ghostdag::CompactGhostdagData,
            headers::HeaderStoreReader,
            past_pruning_points::PastPruningPointsStoreReader,
            pruning::{PruningStore, PruningStoreReader},
            reachability::{DbReachabilityStore, ReachabilityStoreReader, StagingReachabilityStore},
            relations::StagingRelationsStore,
            selected_chain::SelectedChainStore,
            tips::{TipsStore, TipsStoreReader},
            utxo_diffs::UtxoDiffsStoreReader,
            utxo_set::UtxoSetStore,
        },
    },
    processes::{pruning_proof::PruningProofManager, reachability::inquirer as reachability, relations},
};
use crossbeam_channel::Receiver as CrossbeamReceiver;
use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::ORIGIN, blockstatus::BlockStatus::StatusHeaderOnly, muhash::MuHashExtensions, pruning::PruningPointProof, BlockHashSet,
};
use kaspa_consensusmanager::SessionLock;
use kaspa_core::info;
use kaspa_database::prelude::{BatchDbWriter, MemoryWriter, StoreResultExtensions, DB};
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use kaspa_utils::iter::IterExtensions;
use parking_lot::RwLockUpgradableReadGuard;
use rocksdb::WriteBatch;
use std::{collections::VecDeque, ops::Deref, sync::Arc};

pub enum PruningProcessingMessage {
    Exit,
    Process { sink_ghostdag_data: CompactGhostdagData },
}

/// A processor dedicated for moving the pruning point and pruning any possible data in its past
pub struct PruningProcessor {
    // Channels
    receiver: CrossbeamReceiver<PruningProcessingMessage>,

    // DB
    db: Arc<DB>,

    // Storage
    storage: Arc<ConsensusStorage>,

    // Managers and Services
    reachability_service: MTReachabilityService<DbReachabilityStore>,
    ghostdag_managers: Arc<Vec<DbGhostdagManager>>,
    pruning_point_manager: DbPruningPointManager,
    pruning_proof_manager: Arc<PruningProofManager>,

    // Pruning lock
    pruning_lock: SessionLock,
}

impl Deref for PruningProcessor {
    type Target = ConsensusStorage;

    fn deref(&self) -> &Self::Target {
        &self.storage
    }
}

impl PruningProcessor {
    pub fn new(
        receiver: CrossbeamReceiver<PruningProcessingMessage>,
        db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        services: &Arc<ConsensusServices>,
        pruning_lock: SessionLock,
    ) -> Self {
        Self {
            receiver,
            db,
            storage: storage.clone(),
            reachability_service: services.reachability_service.clone(),
            ghostdag_managers: services.ghostdag_managers.clone(),
            pruning_point_manager: services.pruning_point_manager.clone(),
            pruning_proof_manager: services.pruning_proof_manager.clone(),
            pruning_lock,
        }
    }

    pub fn worker(self: &Arc<Self>) {
        while let Ok(mut msg) = self.receiver.recv() {
            let mut exit = false;
            // Empty the channel from all pending messages and process the last one
            for next_msg in self.receiver.try_iter() {
                match next_msg {
                    PruningProcessingMessage::Exit => exit = true,
                    m => msg = m,
                }
            }
            match msg {
                PruningProcessingMessage::Exit => break,
                PruningProcessingMessage::Process { sink_ghostdag_data } => {
                    self.advance_pruning_point_and_candidate_if_possible(sink_ghostdag_data);
                }
            };

            if exit {
                break;
            }
        }
    }

    fn advance_pruning_point_and_candidate_if_possible(&self, sink_ghostdag_data: CompactGhostdagData) {
        let pruning_read_guard = self.pruning_point_store.upgradable_read();
        let current_pruning_info = pruning_read_guard.get().unwrap();
        let (new_pruning_points, new_candidate) = self.pruning_point_manager.next_pruning_points_and_candidate_by_ghostdag_data(
            sink_ghostdag_data,
            None,
            current_pruning_info.candidate,
            current_pruning_info.pruning_point,
        );

        if !new_pruning_points.is_empty() {
            let mut batch = WriteBatch::default();
            let mut write_guard = RwLockUpgradableReadGuard::upgrade(pruning_read_guard);
            for (i, past_pp) in new_pruning_points.iter().copied().enumerate() {
                self.past_pruning_points_store.insert_batch(&mut batch, current_pruning_info.index + i as u64 + 1, past_pp).unwrap();
            }
            let new_pp_index = current_pruning_info.index + new_pruning_points.len() as u64;
            let new_pruning_point = *new_pruning_points.last().unwrap();
            write_guard.set_batch(&mut batch, new_pruning_point, new_candidate, new_pp_index).unwrap();
            self.db.write(batch).unwrap();
            drop(write_guard);

            info!("Daily pruning point movement: advancing from {} to {}", current_pruning_info.pruning_point, new_pruning_point);

            // TODO: DB batching via marker
            let mut utxoset_write = self.pruning_point_utxo_set_store.write();
            for chain_block in
                self.reachability_service.forward_chain_iterator(current_pruning_info.pruning_point, new_pruning_point, true).skip(1)
            {
                let utxo_diff = self.utxo_diffs_store.get(chain_block).expect("chain blocks have utxo state");
                utxoset_write.write_diff(utxo_diff.as_ref()).unwrap();
            }
            drop(utxoset_write);

            // TODO: remove assertion when we stabilize
            self.assert_utxo_commitment(new_pruning_point);

            // Finally, prune data in the new pruning point past
            self.prune(new_pruning_point);
        } else if new_candidate != current_pruning_info.candidate {
            let mut write_guard = RwLockUpgradableReadGuard::upgrade(pruning_read_guard);
            write_guard.set(current_pruning_info.pruning_point, new_candidate, current_pruning_info.index).unwrap();
        }
    }

    fn assert_utxo_commitment(&self, pruning_point: Hash) {
        let commitment = self.headers_store.get_header(pruning_point).unwrap().utxo_commitment;
        let mut multiset = MuHash::new();
        let utxoset_read = self.pruning_point_utxo_set_store.read();
        for (outpoint, entry) in utxoset_read.iterator().map(|r| r.unwrap()) {
            multiset.add_utxo(&outpoint, &entry);
        }
        assert_eq!(multiset.finalize(), commitment, "pruning point utxo set does not match the header utxo commitment");
    }

    fn prune(&self, new_pruning_point: Hash) {
        // TODO: check if archival

        let proof = self.pruning_proof_manager.get_pruning_point_proof();
        let data = self
            .pruning_proof_manager
            .get_pruning_point_anticone_and_trusted_data()
            .expect("insufficient depth error is unexpected here");

        let genesis = self.past_pruning_points_store.get(0).unwrap(); // TODO: pass genesis

        assert_eq!(new_pruning_point, proof[0].last().unwrap().hash);
        assert_eq!(new_pruning_point, data.anticone[0]);
        assert_eq!(genesis, proof.last().unwrap().last().unwrap().hash);

        // We keep full data for pruning point and its anticone, relations for DAA/GD
        // windows and pruning proof, and only headers for past pruning points
        let keep_blocks: BlockHashSet = data.anticone.iter().copied().collect();
        let keep_relations: BlockHashSet = std::iter::empty()
            .chain(data.daa_window_blocks.iter().map(|th| th.header.hash))
            .chain(data.ghostdag_blocks.iter().map(|gd| gd.hash))
            .chain(proof.iter().flatten().map(|h| h.hash))
            .collect();
        let keep_headers: BlockHashSet = self.past_pruning_points();

        info!("Starting Header and Block pruning...");

        let mut prune_guard = self.pruning_lock.blocking_write();
        let mut reachability_read = self.reachability_store.upgradable_read();

        {
            // Start with a batch for pruning body tips and selected chain stores
            let mut batch = WriteBatch::default();

            // Prune tips which can no longer be merged by virtual.
            // By the prunality proof, any tip which isn't in future(pruning_point) will never be merged
            // by virtual and hence can be safely deleted
            let mut tips_write = self.body_tips_store.write();
            let pruned_tips = tips_write
                .get()
                .unwrap()
                .iter()
                .copied()
                .filter(|&h| !reachability_read.is_dag_ancestor_of_result(new_pruning_point, h).unwrap())
                .collect_vec();
            tips_write.prune_tips_with_writer(BatchDbWriter::new(&mut batch), &pruned_tips).unwrap();
            if !pruned_tips.is_empty() {
                info!("Header and Block pruning: pruned {} tips: {:?}", pruned_tips.len(), pruned_tips)
            }

            // Prune the selected chain index below the pruning point
            let mut selected_chain_write = self.selected_chain_store.write();
            selected_chain_write.prune_below_pruning_point(BatchDbWriter::new(&mut batch), new_pruning_point).unwrap();

            // Flush the batch to the DB
            self.db.write(batch).unwrap();

            // Calling the drops explicitly after the batch is written in order to avoid possible errors.
            drop(selected_chain_write);
            drop(tips_write);
        }

        // Now we traverse the anti-future of the new pruning point starting from origin and going up.
        // The most efficient way to traverse the entire DAG from the bottom-up is via the reachability tree
        let mut queue = VecDeque::<Hash>::from_iter(reachability_read.get_children(ORIGIN).unwrap().iter().copied());
        let (mut counter, mut traversed) = (0, 0);
        info!("Header and Block pruning: starting traversal from: {} (genesis: {})", queue.iter().reusable_format(", "), genesis);
        while let Some(current) = queue.pop_front() {
            if reachability_read.is_dag_ancestor_of_result(new_pruning_point, current).unwrap() {
                continue;
            }
            traversed += 1;
            // Obtain the tree children of `current` and push them to the queue before possibly being deleted below
            queue.extend(reachability_read.get_children(current).unwrap().iter());

            if traversed % 50 == 0 {
                // Release and recapture to allow consensus progress during pruning
                drop(prune_guard);
                drop(reachability_read);
                if traversed % 1000 == 0 {
                    info!("Header and Block pruning: traversed: {}, pruned {}...", traversed, counter);
                }
                std::thread::yield_now();
                prune_guard = self.pruning_lock.blocking_write();
                reachability_read = self.reachability_store.upgradable_read();
            }

            // Remove window cache entries
            self.block_window_cache_for_difficulty.remove(&current);
            self.block_window_cache_for_past_median_time.remove(&current);

            if !keep_blocks.contains(&current) {
                let mut batch = WriteBatch::default();
                let mut level_relations_write = self.relations_stores.write();
                let mut staging_relations = StagingRelationsStore::new(self.reachability_relations_store.upgradable_read());
                let mut staging_reachability = StagingReachabilityStore::new(reachability_read);
                let mut statuses_write = self.statuses_store.write();

                // Prune data related to block bodies and UTXO state
                self.utxo_multisets_store.delete_batch(&mut batch, current).unwrap();
                self.utxo_diffs_store.delete_batch(&mut batch, current).unwrap();
                self.acceptance_data_store.delete_batch(&mut batch, current).unwrap();
                self.block_transactions_store.delete_batch(&mut batch, current).unwrap();
                self.daa_excluded_store.delete_batch(&mut batch, current).unwrap();

                if keep_relations.contains(&current) {
                    statuses_write.set_batch(&mut batch, current, StatusHeaderOnly).unwrap();
                } else {
                    // Count only blocks which get fully pruned including DAG relations
                    counter += 1;
                    // Prune data related to headers: relations, reachability, ghostdag
                    let mergeset = relations::delete_reachability_relations(
                        MemoryWriter::default(), // Both stores are staging so we just pass a dummy writer
                        &mut staging_relations,
                        &staging_reachability,
                        current,
                    );
                    reachability::delete_block(&mut staging_reachability, current, &mut mergeset.iter().copied()).unwrap();
                    let block_level = self.headers_store.get_header_with_block_level(current).unwrap().block_level;
                    (0..=block_level as usize).for_each(|level| {
                        relations::delete_level_relations(BatchDbWriter::new(&mut batch), &mut level_relations_write[level], current)
                            .unwrap_option();
                        self.ghostdag_stores[level].delete_batch(&mut batch, current).unwrap_option();
                    });

                    // Remove status completely
                    statuses_write.delete_batch(&mut batch, current).unwrap();

                    if !keep_headers.contains(&current) {
                        // Prune headers
                        self.headers_store.delete_batch(&mut batch, current).unwrap();
                    }
                }

                let reachability_write = staging_reachability.commit(&mut batch).unwrap();
                let reachability_relations_write = staging_relations.commit(&mut batch).unwrap();

                // Flush the batch to the DB
                self.db.write(batch).unwrap();

                // Calling the drops explicitly after the batch is written in order to avoid possible errors.
                drop(reachability_write);
                drop(statuses_write);
                drop(reachability_relations_write);
                drop(level_relations_write);

                reachability_read = self.reachability_store.upgradable_read();
            }
        }
        drop(reachability_read);
        drop(prune_guard);

        info!("Header and Block pruning completed: traversed: {}, pruned {}", traversed, counter);
        info!(
            "Header and Block pruning stats: proof size: {}, pruning point and anticone: {}, unique headers in proof and windows: {}, pruning points in history: {}",
            proof.iter().map(|l| l.len()).sum::<usize>(),
            keep_blocks.len(),
            keep_relations.len(),
            keep_headers.len()
        );

        // TODO: remove this sanity test when stable
        self.assert_proof_rebuilding(proof, new_pruning_point);
    }

    fn past_pruning_points(&self) -> BlockHashSet {
        (0..self.pruning_point_store.read().get().unwrap().index)
            .map(|index| self.past_pruning_points_store.get(index).unwrap())
            .collect()
    }

    fn assert_proof_rebuilding(&self, ref_proof: Arc<PruningPointProof>, new_pruning_point: Hash) {
        info!("Rebuilding the pruning proof after pruning data (Alpha sanity test)");
        let proof_hashes = ref_proof.iter().flatten().map(|h| h.hash).collect::<Vec<_>>();
        let built_proof = self.pruning_proof_manager.build_pruning_point_proof(new_pruning_point);
        let built_proof_hashes = built_proof.iter().flatten().map(|h| h.hash).collect::<Vec<_>>();
        assert_eq!(proof_hashes.len(), built_proof_hashes.len(), "Rebuilt proof does not match the expected reference");
        for (i, (a, b)) in proof_hashes.into_iter().zip(built_proof_hashes).enumerate() {
            if a != b {
                panic!("Proof built following pruning does not match the previous proof: built[{}]={}, prev[{}]={}", i, b, i, a);
            }
        }
        info!("Proof was rebuilt successfully following pruning");
    }
}
