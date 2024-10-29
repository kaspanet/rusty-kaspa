//! TODO: module comment about locking safety and consistency of various pruning stores

use crate::{
    consensus::{
        services::{ConsensusServices, DbParentsManager, DbPruningPointManager},
        storage::ConsensusStorage,
    },
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            ghostdag::{CompactGhostdagData, GhostdagStoreReader},
            headers::HeaderStoreReader,
            past_pruning_points::PastPruningPointsStoreReader,
            pruning::{PruningStore, PruningStoreReader},
            reachability::{DbReachabilityStore, ReachabilityStoreReader, StagingReachabilityStore},
            relations::StagingRelationsStore,
            selected_chain::SelectedChainStore,
            statuses::StatusesStoreReader,
            tips::{TipsStore, TipsStoreReader},
            utxo_diffs::UtxoDiffsStoreReader,
        },
    },
    processes::{pruning_proof::PruningProofManager, reachability::inquirer as reachability, relations},
};
use crossbeam_channel::Receiver as CrossbeamReceiver;
use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::ORIGIN,
    blockstatus::BlockStatus::StatusHeaderOnly,
    config::Config,
    muhash::MuHashExtensions,
    pruning::{PruningPointProof, PruningPointTrustedData},
    trusted::ExternalGhostdagData,
    BlockHashMap, BlockHashSet, BlockLevel,
};
use kaspa_consensusmanager::SessionLock;
use kaspa_core::{debug, info, warn};
use kaspa_database::prelude::{BatchDbWriter, MemoryWriter, StoreResultExtensions, DB};
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use kaspa_utils::iter::IterExtensions;
use parking_lot::RwLockUpgradableReadGuard;
use rocksdb::WriteBatch;
use std::{
    collections::{hash_map::Entry::Vacant, VecDeque},
    ops::Deref,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

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
    pruning_point_manager: DbPruningPointManager,
    pruning_proof_manager: Arc<PruningProofManager>,
    parents_manager: DbParentsManager,

    // Pruning lock
    pruning_lock: SessionLock,

    // Config
    config: Arc<Config>,

    // Signals
    is_consensus_exiting: Arc<AtomicBool>,
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
        config: Arc<Config>,
        is_consensus_exiting: Arc<AtomicBool>,
    ) -> Self {
        Self {
            receiver,
            db,
            storage: storage.clone(),
            reachability_service: services.reachability_service.clone(),
            pruning_point_manager: services.pruning_point_manager.clone(),
            pruning_proof_manager: services.pruning_proof_manager.clone(),
            parents_manager: services.parents_manager.clone(),
            pruning_lock,
            config,
            is_consensus_exiting,
        }
    }

    pub fn worker(self: &Arc<Self>) {
        let Ok(PruningProcessingMessage::Process { sink_ghostdag_data }) = self.receiver.recv() else {
            return;
        };

        // On start-up, check if any pruning workflows require recovery. We wait for the first processing message to arrive
        // in order to make sure the node is already connected and receiving blocks before we start background recovery operations
        self.recover_pruning_workflows_if_needed();
        self.advance_pruning_point_and_candidate_if_possible(sink_ghostdag_data);

        while let Ok(PruningProcessingMessage::Process { sink_ghostdag_data }) = self.receiver.recv() {
            self.advance_pruning_point_and_candidate_if_possible(sink_ghostdag_data);
        }
    }

    fn recover_pruning_workflows_if_needed(&self) {
        let pruning_point_read = self.pruning_point_store.read();
        let pruning_point = pruning_point_read.pruning_point().unwrap();
        let history_root = pruning_point_read.history_root().unwrap_option();
        let pruning_utxoset_position = self.pruning_utxoset_stores.read().utxoset_position().unwrap_option();
        drop(pruning_point_read);

        debug!(
            "[PRUNING PROCESSOR] recovery check: current pruning point: {}, history root: {:?}, pruning utxoset position: {:?}",
            pruning_point, history_root, pruning_utxoset_position
        );

        if let Some(pruning_utxoset_position) = pruning_utxoset_position {
            // This indicates the node crashed during a former pruning point move and we need to recover
            if pruning_utxoset_position != pruning_point {
                info!("Recovering pruning utxo-set from {} to the pruning point {}", pruning_utxoset_position, pruning_point);
                if !self.advance_pruning_utxoset(pruning_utxoset_position, pruning_point) {
                    info!("Interrupted while advancing the pruning point UTXO set: Process is exiting");
                    return;
                }
            }
        }

        if let Some(history_root) = history_root {
            // This indicates the node crashed or was forced to stop during a former data prune operation hence
            // we need to complete it
            if history_root != pruning_point {
                self.prune(pruning_point);
            }
        }

        // TODO: both `pruning_utxoset_position` and `history_root` are new DB keys so for now we assume correct state if the keys are missing
    }

    fn advance_pruning_point_and_candidate_if_possible(&self, sink_ghostdag_data: CompactGhostdagData) {
        let pruning_point_read = self.pruning_point_store.upgradable_read();
        let current_pruning_info = pruning_point_read.get().unwrap();
        let (new_pruning_points, new_candidate) = self.pruning_point_manager.next_pruning_points_and_candidate_by_ghostdag_data(
            sink_ghostdag_data,
            None,
            current_pruning_info.candidate,
            current_pruning_info.pruning_point,
        );

        if !new_pruning_points.is_empty() {
            // Update past pruning points and pruning point stores
            let mut batch = WriteBatch::default();
            let mut pruning_point_write = RwLockUpgradableReadGuard::upgrade(pruning_point_read);
            for (i, past_pp) in new_pruning_points.iter().copied().enumerate() {
                self.past_pruning_points_store.insert_batch(&mut batch, current_pruning_info.index + i as u64 + 1, past_pp).unwrap();
            }
            let new_pp_index = current_pruning_info.index + new_pruning_points.len() as u64;
            let new_pruning_point = *new_pruning_points.last().unwrap();
            pruning_point_write.set_batch(&mut batch, new_pruning_point, new_candidate, new_pp_index).unwrap();
            self.db.write(batch).unwrap();
            drop(pruning_point_write);

            // Inform the user
            info!("Periodic pruning point movement: advancing from {} to {}", current_pruning_info.pruning_point, new_pruning_point);

            // Advance the pruning point utxoset to the state of the new pruning point using chain-block UTXO diffs
            if !self.advance_pruning_utxoset(current_pruning_info.pruning_point, new_pruning_point) {
                info!("Interrupted while advancing the pruning point UTXO set: Process is exiting");
                return;
            }
            info!("Updated the pruning point UTXO set");

            // Finally, prune data in the new pruning point past
            self.prune(new_pruning_point);
        } else if new_candidate != current_pruning_info.candidate {
            let mut pruning_point_write = RwLockUpgradableReadGuard::upgrade(pruning_point_read);
            pruning_point_write.set(current_pruning_info.pruning_point, new_candidate, current_pruning_info.index).unwrap();
        }
    }

    fn advance_pruning_utxoset(&self, utxoset_position: Hash, new_pruning_point: Hash) -> bool {
        let mut pruning_utxoset_write = self.pruning_utxoset_stores.write();
        for chain_block in self.reachability_service.forward_chain_iterator(utxoset_position, new_pruning_point, true).skip(1) {
            if self.is_consensus_exiting.load(Ordering::Relaxed) {
                return false;
            }
            let utxo_diff = self.utxo_diffs_store.get(chain_block).expect("chain blocks have utxo state");
            let mut batch = WriteBatch::default();
            pruning_utxoset_write.utxo_set.write_diff_batch(&mut batch, utxo_diff.as_ref()).unwrap();
            pruning_utxoset_write.set_utxoset_position(&mut batch, chain_block).unwrap();
            self.db.write(batch).unwrap();
        }
        drop(pruning_utxoset_write);

        if self.config.enable_sanity_checks {
            info!("Performing a sanity check that the new UTXO set has the expected UTXO commitment");
            self.assert_utxo_commitment(new_pruning_point);
        }
        true
    }

    fn assert_utxo_commitment(&self, pruning_point: Hash) {
        info!("Verifying the new pruning point UTXO commitment (sanity test)");
        let commitment = self.headers_store.get_header(pruning_point).unwrap().utxo_commitment;
        let mut multiset = MuHash::new();
        let pruning_utxoset_read = self.pruning_utxoset_stores.read();
        for (outpoint, entry) in pruning_utxoset_read.utxo_set.iterator().map(|r| r.unwrap()) {
            multiset.add_utxo(&outpoint, &entry);
        }
        assert_eq!(multiset.finalize(), commitment, "Updated pruning point utxo set does not match the header utxo commitment");
        info!("Pruning point UTXO commitment was verified correctly (sanity test)");
    }

    fn prune(&self, new_pruning_point: Hash) {
        if self.config.is_archival {
            warn!("The node is configured as an archival node -- avoiding data pruning. Note this might lead to heavy disk usage.");
            return;
        }

        info!("Header and Block pruning: preparing proof and anticone data...");

        let proof = self.pruning_proof_manager.get_pruning_point_proof();
        let data = self
            .pruning_proof_manager
            .get_pruning_point_anticone_and_trusted_data()
            .expect("insufficient depth error is unexpected here");

        let genesis = self.past_pruning_points_store.get(0).unwrap();

        assert_eq!(new_pruning_point, proof[0].last().unwrap().hash);
        assert_eq!(new_pruning_point, data.anticone[0]);
        assert_eq!(genesis, self.config.genesis.hash);
        assert_eq!(genesis, proof.last().unwrap().last().unwrap().hash);

        // We keep full data for pruning point and its anticone, relations for DAA/GD
        // windows and pruning proof, and only headers for past pruning points
        let keep_blocks: BlockHashSet = data.anticone.iter().copied().collect();
        let mut keep_relations: BlockHashMap<BlockLevel> = std::iter::empty()
            .chain(data.anticone.iter().copied())
            .chain(data.daa_window_blocks.iter().map(|th| th.header.hash))
            .chain(data.ghostdag_blocks.iter().map(|gd| gd.hash))
            .chain(proof[0].iter().map(|h| h.hash))
            .map(|h| (h, 0)) // Mark block level 0 for all the above. Note that below we add the remaining levels
            .collect();
        let keep_headers: BlockHashSet = self.past_pruning_points();

        info!("Header and Block pruning: waiting for consensus write permissions...");

        let mut prune_guard = self.pruning_lock.blocking_write();

        info!("Starting Header and Block pruning...");

        {
            let mut counter = 0;
            let mut batch = WriteBatch::default();
            // At this point keep_relations only holds level-0 relations which is the correct filtering criteria for primary GHOSTDAG
            for kept in keep_relations.keys().copied() {
                let Some(ghostdag) = self.ghostdag_store.get_data(kept).unwrap_option() else {
                    continue;
                };
                if ghostdag.unordered_mergeset().any(|h| !keep_relations.contains_key(&h)) {
                    let mut mutable_ghostdag: ExternalGhostdagData = ghostdag.as_ref().into();
                    mutable_ghostdag.mergeset_blues.retain(|h| keep_relations.contains_key(h));
                    mutable_ghostdag.mergeset_reds.retain(|h| keep_relations.contains_key(h));
                    mutable_ghostdag.blues_anticone_sizes.retain(|k, _| keep_relations.contains_key(k));
                    if !keep_relations.contains_key(&mutable_ghostdag.selected_parent) {
                        mutable_ghostdag.selected_parent = ORIGIN;
                    }
                    counter += 1;
                    self.ghostdag_store.update_batch(&mut batch, kept, &Arc::new(mutable_ghostdag.into())).unwrap();
                }
            }
            self.db.write(batch).unwrap();
            info!("Header and Block pruning: updated ghostdag data for {} blocks", counter);
        }

        // No need to hold the prune guard while we continue populating keep_relations
        drop(prune_guard);

        // Add additional levels only after filtering GHOSTDAG data via level 0
        for (level, level_proof) in proof.iter().enumerate().skip(1) {
            let level = level as BlockLevel;
            // We obtain the headers of the pruning point anticone (including the pruning point)
            // in order to mark all parents of anticone roots at level as not-to-be-deleted.
            // This optimizes multi-level parent validation (see ParentsManager)
            // by avoiding the deletion of high-level parents which might still be needed for future
            // header validation (avoiding the need for reference blocks; see therein).
            //
            // Notes:
            //
            // 1. Normally, such blocks would be part of the proof for this level, but here we address the rare case
            //    where there are a few such parallel blocks (since the proof only contains the past of the pruning point's
            //    selected-tip-at-level)
            // 2. We refer to the pp anticone as roots even though technically it might contain blocks which are not a pure
            //    antichain (i.e., some of them are in the past of others). These blocks only add redundant info which would
            //    be included anyway.
            let roots_parents_at_level = data
                .anticone
                .iter()
                .copied()
                .map(|hash| self.headers_store.get_header_with_block_level(hash).expect("pruning point anticone is not pruned"))
                .filter(|root| level > root.block_level) // If the root itself is at level, there's no need for its level-parents
                .flat_map(|root| self.parents_manager.parents_at_level(&root.header, level).iter().copied().collect_vec());
            for hash in level_proof.iter().map(|header| header.hash).chain(roots_parents_at_level) {
                if let Vacant(e) = keep_relations.entry(hash) {
                    // This hash was not added by any lower level -- mark it as affiliated with proof level `level`
                    e.insert(level);
                }
            }
        }

        prune_guard = self.pruning_lock.blocking_write();
        let mut lock_acquire_time = Instant::now();
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
                .read()
                .iter()
                .copied()
                .filter(|&h| !reachability_read.is_dag_ancestor_of_result(new_pruning_point, h).unwrap())
                .collect_vec();
            tips_write.prune_tips_with_writer(BatchDbWriter::new(&mut batch), &pruned_tips).unwrap();
            if !pruned_tips.is_empty() {
                info!(
                    "Header and Block pruning: pruned {} tips: {}...{}",
                    pruned_tips.len(),
                    pruned_tips.iter().take(5.min((pruned_tips.len() + 1) / 2)).reusable_format(", "),
                    pruned_tips.iter().rev().take(5.min(pruned_tips.len() / 2)).reusable_format(", ")
                )
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

            // If we have the lock for more than a few milliseconds, release and recapture to allow consensus progress during pruning
            if lock_acquire_time.elapsed() > Duration::from_millis(5) {
                drop(reachability_read);
                // An exit signal was received. Exit from this long running process.
                if self.is_consensus_exiting.load(Ordering::Relaxed) {
                    drop(prune_guard);
                    info!("Header and Block pruning interrupted: Process is exiting");
                    return;
                }
                prune_guard.blocking_yield();
                lock_acquire_time = Instant::now();
                reachability_read = self.reachability_store.upgradable_read();
            }

            if traversed % 1000 == 0 {
                info!("Header and Block pruning: traversed: {}, pruned {}...", traversed, counter);
            }

            // Remove window cache entries
            self.block_window_cache_for_difficulty.remove(&current);
            self.block_window_cache_for_past_median_time.remove(&current);

            if !keep_blocks.contains(&current) {
                let mut batch = WriteBatch::default();
                let mut level_relations_write = self.relations_stores.write();
                let mut reachability_relations_write = self.reachability_relations_store.write();
                let mut staging_relations = StagingRelationsStore::new(&mut reachability_relations_write);
                let mut staging_reachability = StagingReachabilityStore::new(reachability_read);
                let mut statuses_write = self.statuses_store.write();

                // Prune data related to block bodies and UTXO state
                self.utxo_multisets_store.delete_batch(&mut batch, current).unwrap();
                self.utxo_diffs_store.delete_batch(&mut batch, current).unwrap();
                self.acceptance_data_store.delete_batch(&mut batch, current).unwrap();
                self.block_transactions_store.delete_batch(&mut batch, current).unwrap();

                if let Some(&affiliated_proof_level) = keep_relations.get(&current) {
                    if statuses_write.get(current).unwrap_option().is_some_and(|s| s.is_valid()) {
                        // We set the status to header-only only if it was previously set to a valid
                        // status. This is important since some proof headers might not have their status set
                        // and we would like to preserve this semantic (having a valid status implies that
                        // other parts of the code assume the existence of GD data etc.)
                        statuses_write.set_batch(&mut batch, current, StatusHeaderOnly).unwrap();
                    }

                    // Delete level-x relations for blocks which only belong to higher-than-x proof levels.
                    // This preserves the semantic that for each level, relations represent a contiguous DAG area in that level
                    for lower_level in 0..affiliated_proof_level as usize {
                        let mut staging_level_relations = StagingRelationsStore::new(&mut level_relations_write[lower_level]);
                        relations::delete_level_relations(MemoryWriter, &mut staging_level_relations, current).unwrap_option();
                        staging_level_relations.commit(&mut batch).unwrap();

                        if lower_level == 0 {
                            self.ghostdag_store.delete_batch(&mut batch, current).unwrap_option();
                        }
                    }
                } else {
                    // Count only blocks which get fully pruned including DAG relations
                    counter += 1;
                    // Prune data related to headers: relations, reachability, ghostdag
                    let mergeset = relations::delete_reachability_relations(
                        MemoryWriter, // Both stores are staging so we just pass a dummy writer
                        &mut staging_relations,
                        &staging_reachability,
                        current,
                    );
                    reachability::delete_block(&mut staging_reachability, current, &mut mergeset.iter().copied()).unwrap();
                    // TODO: consider adding block level to compact header data
                    let block_level = self.headers_store.get_header_with_block_level(current).unwrap().block_level;
                    (0..=block_level as usize).for_each(|level| {
                        let mut staging_level_relations = StagingRelationsStore::new(&mut level_relations_write[level]);
                        relations::delete_level_relations(MemoryWriter, &mut staging_level_relations, current).unwrap_option();
                        staging_level_relations.commit(&mut batch).unwrap();
                    });

                    self.ghostdag_store.delete_batch(&mut batch, current).unwrap_option();

                    // Remove additional header related data
                    self.daa_excluded_store.delete_batch(&mut batch, current).unwrap();
                    self.depth_store.delete_batch(&mut batch, current).unwrap();
                    // Remove status completely
                    statuses_write.delete_batch(&mut batch, current).unwrap();

                    if !keep_headers.contains(&current) {
                        // Prune the actual headers
                        self.headers_store.delete_batch(&mut batch, current).unwrap();
                    }
                }

                let reachability_write = staging_reachability.commit(&mut batch).unwrap();
                staging_relations.commit(&mut batch).unwrap();

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

        if self.config.enable_sanity_checks {
            self.assert_proof_rebuilding(proof, new_pruning_point);
            self.assert_data_rebuilding(data, new_pruning_point);
        }

        {
            // Set the history root to the new pruning point only after we successfully pruned its past
            let mut pruning_point_write = self.pruning_point_store.write();
            let mut batch = WriteBatch::default();
            pruning_point_write.set_history_root(&mut batch, new_pruning_point).unwrap();
            self.db.write(batch).unwrap();
            drop(pruning_point_write);
        }
    }

    fn past_pruning_points(&self) -> BlockHashSet {
        (0..self.pruning_point_store.read().get().unwrap().index)
            .map(|index| self.past_pruning_points_store.get(index).unwrap())
            .collect()
    }

    fn assert_proof_rebuilding(&self, ref_proof: Arc<PruningPointProof>, new_pruning_point: Hash) {
        info!("Rebuilding the pruning proof after pruning data (sanity test)");
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

    fn assert_data_rebuilding(&self, ref_data: Arc<PruningPointTrustedData>, new_pruning_point: Hash) {
        info!("Rebuilding pruning point trusted data (sanity test)");
        let virtual_state = self.lkg_virtual_state.load();
        let built_data = self
            .pruning_proof_manager
            .calculate_pruning_point_anticone_and_trusted_data(new_pruning_point, virtual_state.parents.iter().copied());
        assert_eq!(
            ref_data.anticone.iter().copied().collect::<BlockHashSet>(),
            built_data.anticone.iter().copied().collect::<BlockHashSet>()
        );
        assert_eq!(
            ref_data.daa_window_blocks.iter().map(|th| th.header.hash).collect::<BlockHashSet>(),
            built_data.daa_window_blocks.iter().map(|th| th.header.hash).collect::<BlockHashSet>()
        );
        assert_eq!(
            ref_data.ghostdag_blocks.iter().map(|gd| gd.hash).collect::<BlockHashSet>(),
            built_data.ghostdag_blocks.iter().map(|gd| gd.hash).collect::<BlockHashSet>()
        );
        info!("Trusted data was rebuilt successfully following pruning");
    }
}
