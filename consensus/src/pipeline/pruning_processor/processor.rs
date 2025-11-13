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
            pruning::PruningStoreReader,
            pruning_samples::PruningSamplesStoreReader,
            reachability::{DbReachabilityStore, ReachabilityStoreReader, StagingReachabilityStore},
            relations::StagingRelationsStore,
            selected_chain::{SelectedChainStore, SelectedChainStoreReader},
            statuses::StatusesStoreReader,
            tips::{TipsStore, TipsStoreReader},
            utxo_diffs::UtxoDiffsStoreReader,
            virtual_state::VirtualStateStoreReader,
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
use kaspa_core::{debug, info, trace, warn};
use kaspa_database::prelude::{BatchDbWriter, MemoryWriter, StoreResultExtensions, DB};
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use kaspa_utils::iter::IterExtensions;
use parking_lot::RwLockUpgradableReadGuard;
use rocksdb::WriteBatch;
use std::{
    collections::{hash_map::Entry::Vacant, BTreeMap, VecDeque},
    mem,
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

#[derive(Default)]
struct CommitStats {
    commits: usize,
    total_ops: usize,
    max_ops: usize,
    total_bytes: usize,
    max_bytes: usize,
    total_duration: Duration,
    max_duration: Duration,
}

impl CommitStats {
    fn record(&mut self, ops: usize, bytes: usize, duration: Duration) {
        self.commits += 1;
        self.total_ops += ops;
        self.max_ops = self.max_ops.max(ops);
        self.total_bytes += bytes;
        self.max_bytes = self.max_bytes.max(bytes);
        self.total_duration += duration;
        self.max_duration = self.max_duration.max(duration);
    }
}

struct PruningPhaseMetrics {
    started: Instant,
    commit_stats: BTreeMap<&'static str, CommitStats>,
    total_traversed: usize,
    total_pruned: usize,
    total_lock_hold: Duration,
    lock_yield_count: usize,
    lock_reacquire_count: usize,
}

impl PruningPhaseMetrics {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            commit_stats: BTreeMap::new(),
            total_traversed: 0,
            total_pruned: 0,
            total_lock_hold: Duration::ZERO,
            lock_yield_count: 0,
            lock_reacquire_count: 0,
        }
    }

    fn record_commit(&mut self, context: &'static str, ops: usize, bytes: usize, duration: Duration) {
        self.commit_stats.entry(context).or_default().record(ops, bytes, duration);
    }

    fn record_lock_yield(&mut self, held: Duration) {
        self.total_lock_hold += held;
        self.lock_yield_count += 1;
    }

    fn finalize_lock_hold(&mut self, held: Duration) {
        self.total_lock_hold += held;
    }

    fn record_lock_reacquire(&mut self) {
        self.lock_reacquire_count += 1;
    }

    fn set_traversed(&mut self, traversed: usize, pruned: usize) {
        self.total_traversed = traversed;
        self.total_pruned = pruned;
    }

    fn log_summary(&self) {
        let elapsed_ms = self.started.elapsed().as_millis();
        info!(
            "[PRUNING METRICS] config_lock_max_ms={} config_batch_max_ms={} config_batch_max_ops={} config_batch_max_bytes={} duration_ms={} traversed={} pruned={} lock_hold_ms={} lock_yields={} lock_reacquires={}",
            PRUNE_LOCK_MAX_DURATION_MS,
            PRUNE_BATCH_MAX_DURATION_MS,
            PRUNE_BATCH_MAX_OPS,
            PRUNE_BATCH_MAX_BYTES,
            elapsed_ms,
            self.total_traversed,
            self.total_pruned,
            self.total_lock_hold.as_millis(),
            self.lock_yield_count,
            self.lock_reacquire_count
        );
        for (context, stats) in &self.commit_stats {
            let avg_ops = if stats.commits == 0 { 0.0 } else { stats.total_ops as f64 / stats.commits as f64 };
            let avg_bytes = if stats.commits == 0 { 0.0 } else { stats.total_bytes as f64 / stats.commits as f64 };
            let avg_duration_ms =
                if stats.commits == 0 { 0.0 } else { stats.total_duration.as_secs_f64() * 1000.0 / stats.commits as f64 };
            info!(
                "[PRUNING METRICS] commit_type={} count={} avg_ops={:.2} max_ops={} avg_bytes={:.2} max_bytes={} avg_commit_ms={:.3} max_commit_ms={:.3}",
                context,
                stats.commits,
                avg_ops,
                stats.max_ops,
                avg_bytes,
                stats.max_bytes,
                avg_duration_ms,
                stats.max_duration.as_secs_f64() * 1000.0
            );
        }
    }
}

const PRUNE_BATCH_MAX_BLOCKS: usize = 256;
const PRUNE_BATCH_MAX_OPS: usize = 50_000;
const PRUNE_BATCH_MAX_BYTES: usize = 4 * 1024 * 1024;
const PRUNE_BATCH_MAX_DURATION_MS: u64 = 50;
const PRUNE_LOCK_MAX_DURATION_MS: u64 = 25;

struct PruneBatch {
    batch: WriteBatch,
    block_count: usize,
    started: Option<Instant>,
}

impl PruneBatch {
    fn new() -> Self {
        Self { batch: WriteBatch::default(), block_count: 0, started: None }
    }

    fn on_block_staged(&mut self) {
        if self.block_count == 0 {
            self.started = Some(Instant::now());
        }
        self.block_count += 1;
    }

    fn len(&self) -> usize {
        self.batch.len()
    }

    fn size_in_bytes(&self) -> usize {
        self.batch.size_in_bytes()
    }

    fn blocks(&self) -> usize {
        self.block_count
    }

    fn elapsed(&self) -> Duration {
        self.started.map(|t| t.elapsed()).unwrap_or_default()
    }

    fn is_empty(&self) -> bool {
        self.batch.len() == 0
    }

    fn take(&mut self) -> WriteBatch {
        self.block_count = 0;
        self.started = None;
        mem::take(&mut self.batch)
    }
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
        // On start-up, check if any pruning workflows require recovery. We wait for the first processing message to arrive
        // in order to make sure the node is already connected and receiving blocks before we start background recovery operations
        let mut recovered = false;
        while let Ok(PruningProcessingMessage::Process { sink_ghostdag_data }) = self.receiver.recv() {
            if !recovered {
                if !self.recover_pruning_workflows_if_needed() {
                    // Recovery could fail for several reasons:
                    // (a) Consensus has exited while it was undergoing
                    // (b) Consensus is in a transitional state
                    // (c) Consensus is no longer in a transitional state per-se but has yet to catch up on sufficient block data
                    // For (a), the best course of measure is to exit the loop
                    // For (b)+(c), it is to attempt it again
                    // Continuing the loop satisfies both since if consensus exited the next iteration of the loop will exit as well
                    continue;
                }
                recovered = true;
            }
            self.advance_pruning_point_and_candidate_if_possible(sink_ghostdag_data);
        }
    }

    fn recover_pruning_workflows_if_needed(&self) -> bool {
        // returns true if recorvery was completed successfully or was not needed
        let pruning_point_read = self.pruning_point_store.read();
        let pruning_point = pruning_point_read.pruning_point().unwrap();
        let retention_checkpoint = pruning_point_read.retention_checkpoint().unwrap();
        let retention_period_root = pruning_point_read.retention_period_root().unwrap();
        let pruning_meta_read = self.pruning_meta_stores.read();
        let pruning_utxoset_position = pruning_meta_read.utxoset_position().unwrap();
        drop(pruning_point_read);
        drop(pruning_meta_read);

        debug!(
            "[PRUNING PROCESSOR] recovery check: current pruning point: {}, retention checkpoint: {:?}, pruning utxoset position: {:?}",
            pruning_point, retention_checkpoint, pruning_utxoset_position
        );

        // This indicates the node crashed during a former pruning point move and we need to recover
        if pruning_utxoset_position != pruning_point {
            info!("Recovering pruning utxo-set from {} to the pruning point {}", pruning_utxoset_position, pruning_point);
            if !self.advance_pruning_utxoset(pruning_utxoset_position, pruning_point) {
                info!("Interrupted while advancing the pruning point UTXO set: Process is exiting");
                return false;
            }
        }
        // The following two chekcs are implicitly checked in advance_pruning_utxoset, and hence can theoretically
        // be skipped if that function was called. As these checks are cheap, we  perform them regardless
        // as to not complicate the logic.

        // If the latest pruning point is the result of an IBD catchup, it is guaranteed that the headers selected tip
        // is pruning_depth on top of it
        // but crucially it is not guaranteed *virtual* is of sufficient depth above it
        // internally the pruning process checks this process for virtual and fails otherwise
        // for this reason, pruning is held until virtual has advanced enough.
        if !self.confirm_pruning_depth_below_virtual(pruning_point) {
            return false;
        }
        let pruning_meta_read = self.pruning_meta_stores.read();

        // don't prune if in a transitional ibd state.
        if pruning_meta_read.is_in_transitional_ibd_state() {
            return false;
        }

        drop(pruning_meta_read);
        trace!(
            "retention_checkpoint: {:?} | retention_period_root: {} | pruning_point: {}",
            retention_checkpoint,
            retention_period_root,
            pruning_point
        );

        // This indicates the node crashed or was forced to stop during a former data prune operation hence
        // we need to complete it
        if retention_checkpoint != retention_period_root {
            self.prune(pruning_point, retention_period_root);
        }
        true
    }

    fn advance_pruning_point_and_candidate_if_possible(&self, sink_ghostdag_data: CompactGhostdagData) {
        let pruning_point_read = self.pruning_point_store.upgradable_read();
        let (current_pruning_point, current_index) = pruning_point_read.pruning_point_and_index().unwrap();
        let new_pruning_points = self.pruning_point_manager.next_pruning_points(sink_ghostdag_data, current_pruning_point);

        if let Some(new_pruning_point) = new_pruning_points.last().copied() {
            let retention_period_root = pruning_point_read.retention_period_root().unwrap();

            // Update past pruning points and pruning point stores
            let mut batch = WriteBatch::default();
            let mut pruning_point_write = RwLockUpgradableReadGuard::upgrade(pruning_point_read);
            for (i, past_pp) in new_pruning_points.iter().copied().enumerate() {
                self.past_pruning_points_store.insert_batch(&mut batch, current_index + i as u64 + 1, past_pp).unwrap();
            }
            let new_pp_index = current_index + new_pruning_points.len() as u64;
            pruning_point_write.set_batch(&mut batch, new_pruning_point, new_pp_index).unwrap();

            // For archival nodes, keep the retention root in place
            let adjusted_retention_period_root = if self.config.is_archival {
                retention_period_root
            } else {
                let adjusted_retention_period_root = self.advance_retention_period_root(retention_period_root, new_pruning_point);
                pruning_point_write.set_retention_period_root(&mut batch, adjusted_retention_period_root).unwrap();
                adjusted_retention_period_root
            };

            self.db.write(batch).unwrap();
            drop(pruning_point_write);

            trace!("New Pruning Point: {} | New Retention Period Root: {}", new_pruning_point, adjusted_retention_period_root);

            // Inform the user
            info!("Periodic pruning point movement: advancing from {} to {}", current_pruning_point, new_pruning_point);

            // Advance the pruning point utxoset to the state of the new pruning point using chain-block UTXO diffs
            if !self.advance_pruning_utxoset(current_pruning_point, new_pruning_point) {
                info!("Interrupted while advancing the pruning point UTXO set: Process is exiting");
                return;
            }
            info!("Updated the pruning point UTXO set");

            // Finally, prune data in the new pruning point past
            self.prune(new_pruning_point, adjusted_retention_period_root);
        }
    }

    fn advance_pruning_utxoset(&self, utxoset_position: Hash, new_pruning_point: Hash) -> bool {
        // If the latest pruning point is the result of an IBD catchup, it is guaranteed that the headers selected tip
        // is pruning_depth on top of it
        // but crucially it is not guaranteed *virtual* is of sufficient depth above it
        // internally the pruning process checks this process for virtual and fails otherwise
        // for this reason, pruning is held until virtual has advanced enough.
        if !self.confirm_pruning_depth_below_virtual(new_pruning_point) {
            return false;
        }

        for chain_block in self.reachability_service.forward_chain_iterator(utxoset_position, new_pruning_point, true).skip(1) {
            if self.is_consensus_exiting.load(Ordering::Relaxed) {
                return false;
            }
            // halt pruning if an unstable IBD state was initiated in the midst of it
            let pruning_meta_read = self.pruning_meta_stores.upgradable_read();

            if pruning_meta_read.is_in_transitional_ibd_state() {
                return false;
            }
            let mut pruning_meta_write = RwLockUpgradableReadGuard::upgrade(pruning_meta_read);

            let utxo_diff = self.utxo_diffs_store.get(chain_block).expect("chain blocks have utxo state");
            let mut batch = WriteBatch::default();
            pruning_meta_write.utxo_set.write_diff_batch(&mut batch, utxo_diff.as_ref()).unwrap();
            pruning_meta_write.set_utxoset_position(&mut batch, chain_block).unwrap();
            self.db.write(batch).unwrap();
            drop(pruning_meta_write);
        }

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
        let pruning_meta_read = self.pruning_meta_stores.read();
        for (outpoint, entry) in pruning_meta_read.utxo_set.iterator().map(|r| r.unwrap()) {
            multiset.add_utxo(&outpoint, &entry);
        }
        assert_eq!(multiset.finalize(), commitment, "Updated pruning point utxo set does not match the header utxo commitment");
        info!("Pruning point UTXO commitment was verified correctly (sanity test)");
    }

    fn prune(&self, new_pruning_point: Hash, retention_period_root: Hash) {
        if self.config.is_archival {
            warn!("The node is configured as an archival node -- avoiding data pruning. Note this might lead to heavy disk usage.");
            return;
        }

        info!("Header and Block pruning: preparing proof and anticone data...");
        let mut metrics = PruningPhaseMetrics::new();

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
        metrics.record_lock_reacquire();
        let mut lock_acquire_time = Instant::now();

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
            let ops = batch.len();
            let bytes = batch.size_in_bytes();
            let commit_start = Instant::now();
            self.db.write(batch).unwrap();
            metrics.record_commit("ghostdag_adjust", ops, bytes, commit_start.elapsed());
            info!("Header and Block pruning: updated ghostdag data for {} blocks", counter);
        }

        // No need to hold the prune guard while we continue populating keep_relations
        metrics.record_lock_yield(lock_acquire_time.elapsed());
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
        lock_acquire_time = Instant::now();
        metrics.record_lock_reacquire();
        let mut reachability_read = Some(self.reachability_store.upgradable_read());
        let mut prune_batch = PruneBatch::new();

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
                .filter(|&h| {
                    !reachability_read
                        .as_ref()
                        .expect("reachability guard should be available")
                        .is_dag_ancestor_of_result(new_pruning_point, h)
                        .unwrap()
                })
                .collect_vec();
            tips_write.prune_tips_with_writer(BatchDbWriter::new(&mut batch), &pruned_tips).unwrap();
            if !pruned_tips.is_empty() {
                info!(
                    "Header and Block pruning: pruned {} tips: {}...{}",
                    pruned_tips.len(),
                    pruned_tips.iter().take(5.min(pruned_tips.len().div_ceil(2))).reusable_format(", "),
                    pruned_tips.iter().rev().take(5.min(pruned_tips.len() / 2)).reusable_format(", ")
                )
            }

            // Prune the selected chain index below the pruning point
            let mut selected_chain_write = self.selected_chain_store.write();
            // Temp — bug fix upgrade logic: the prev wrong logic might have pruned the new retention period root from the selected chain store,
            //                               hence we verify its existence first and only then proceed.
            // TODO (in upcoming versions): remove this temp condition
            if retention_period_root == new_pruning_point
                || selected_chain_write.get_by_hash(retention_period_root).unwrap_option().is_some()
            {
                selected_chain_write.prune_below_point(BatchDbWriter::new(&mut batch), retention_period_root).unwrap();
            }

            // Flush the batch to the DB
            let ops = batch.len();
            let bytes = batch.size_in_bytes();
            let commit_start = Instant::now();
            self.db.write(batch).unwrap();
            metrics.record_commit("tips_and_selected_chain", ops, bytes, commit_start.elapsed());

            // Calling the drops explicitly after the batch is written in order to avoid possible errors.
            drop(selected_chain_write);
            drop(tips_write);
        }

        // Now we traverse the anti-future of the new pruning point starting from origin and going up.
        // The most efficient way to traverse the entire DAG from the bottom-up is via the reachability tree
        let mut queue = VecDeque::<Hash>::from_iter(
            reachability_read.as_ref().expect("reachability guard should be available").get_children(ORIGIN).unwrap().iter().copied(),
        );
        let (mut counter, mut traversed) = (0, 0);
        info!("Header and Block pruning: starting traversal from: {} (genesis: {})", queue.iter().reusable_format(", "), genesis);

        'staging: loop {
            // Create staging stores once per batch to maintain consistency across multiple block deletions
            let mut level_relations_write = self.relations_stores.write();
            let mut reachability_relations_write = self.reachability_relations_store.write();
            let mut staging_relations = StagingRelationsStore::new(&mut reachability_relations_write);
            let mut staging_reachability =
                StagingReachabilityStore::new(reachability_read.take().expect("reachability guard should be available"));
            let mut statuses_write = self.statuses_store.write();

            while let Some(current) = queue.pop_front() {
                if lock_acquire_time.elapsed() > Duration::from_millis(PRUNE_LOCK_MAX_DURATION_MS) {
                    // Commit staging stores and flush the batch so we can yield
                    let reachability_write = staging_reachability.commit(&mut prune_batch.batch).unwrap();
                    staging_relations.commit(&mut prune_batch.batch).unwrap();
                    drop(reachability_write);
                    drop(statuses_write);
                    drop(reachability_relations_write);
                    drop(level_relations_write);

                    if !prune_batch.is_empty() {
                        self.flush_prune_batch(&mut prune_batch, &mut metrics);
                    }

                    metrics.record_lock_yield(lock_acquire_time.elapsed());
                    // An exit signal was received. Exit from this long running process.
                    if self.is_consensus_exiting.load(Ordering::Relaxed) {
                        drop(prune_guard);
                        info!("Header and Block pruning interrupted: Process is exiting");
                        return;
                    }
                    prune_guard.blocking_yield();
                    lock_acquire_time = Instant::now();
                    queue.push_front(current);
                    reachability_read = Some(self.reachability_store.upgradable_read());
                    metrics.record_lock_reacquire();
                    continue 'staging;
                }

                let skip_due_to_retention = match staging_reachability.is_dag_ancestor_of_result(retention_period_root, current) {
                    Ok(result) => result,
                    Err(err) if err.is_key_not_found() => {
                        // A keyed block might already be staged for deletion in the current batch.
                        // The underlying store still contains it until the batch is flushed, so consult
                        // a fresh read guard to answer the reachability query.
                        let reachability_read_only = self.reachability_store.read();
                        reachability_read_only.is_dag_ancestor_of_result(retention_period_root, current).unwrap()
                    }
                    Err(err) => panic!("Unexpected reachability error while checking retention ancestry: {err:?}"),
                };
                if skip_due_to_retention {
                    continue;
                }
                traversed += 1;
                // Obtain the tree children of `current` and push them to the queue before possibly being deleted below
                queue.extend(staging_reachability.get_children(current).unwrap().iter());

                if traversed % 1000 == 0 {
                    info!("Header and Block pruning: traversed: {}, pruned {}...", traversed, counter);
                }

                // Remove window cache entries
                self.block_window_cache_for_difficulty.remove(&current);
                self.block_window_cache_for_past_median_time.remove(&current);

                if !keep_blocks.contains(&current) {
                    let batch = &mut prune_batch.batch;

                    // Prune data related to block bodies and UTXO state
                    self.utxo_multisets_store.delete_batch(batch, current).unwrap();
                    self.utxo_diffs_store.delete_batch(batch, current).unwrap();
                    self.acceptance_data_store.delete_batch(batch, current).unwrap();
                    self.block_transactions_store.delete_batch(batch, current).unwrap();

                    if let Some(&affiliated_proof_level) = keep_relations.get(&current) {
                        if statuses_write.get(current).unwrap_option().is_some_and(|s| s.is_valid()) {
                            // We set the status to header-only only if it was previously set to a valid
                            // status. This is important since some proof headers might not have their status set
                            // and we would like to preserve this semantic (having a valid status implies that
                            // other parts of the code assume the existence of GD data etc.)
                            statuses_write.set_batch(batch, current, StatusHeaderOnly).unwrap();
                        }

                        // Delete level-x relations for blocks which only belong to higher-than-x proof levels.
                        // This preserves the semantic that for each level, relations represent a contiguous DAG area in that level
                        for lower_level in 0..affiliated_proof_level as usize {
                            let mut staging_level_relations = StagingRelationsStore::new(&mut level_relations_write[lower_level]);
                            relations::delete_level_relations(MemoryWriter, &mut staging_level_relations, current).unwrap_option();
                            staging_level_relations.commit(batch).unwrap();

                            if lower_level == 0 {
                                self.ghostdag_store.delete_batch(batch, current).unwrap_option();
                            }
                        }

                        // While we keep headers for keep-relation blocks regardless, some of those
                        // blocks may still hold pruning samples. Drop those samples unless the block
                        // is itself part of the pruning set we intentionally keep headers for.
                        if !keep_headers.contains(&current) {
                            self.pruning_samples_store.delete_batch(batch, current).unwrap();
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
                            staging_level_relations.commit(batch).unwrap();
                        });

                        self.ghostdag_store.delete_batch(batch, current).unwrap_option();

                        // Remove additional header related data
                        self.daa_excluded_store.delete_batch(batch, current).unwrap();
                        self.depth_store.delete_batch(batch, current).unwrap();
                        // Remove status completely
                        statuses_write.delete_batch(batch, current).unwrap();

                        if !keep_headers.contains(&current) {
                            // Prune the actual headers
                            self.headers_store.delete_batch(batch, current).unwrap();

                            // We want to keep the pruning sample from POV for past pruning points
                            // so that pruning point queries keep working for blocks right after the current
                            // pruning point (keep_headers contains the past pruning points)
                            self.pruning_samples_store.delete_batch(batch, current).unwrap();
                        }
                    }
                    prune_batch.on_block_staged();
                }

                let lock_elapsed = lock_acquire_time.elapsed();
                if self.should_flush_prune_batch(&prune_batch, lock_elapsed) {
                    let reachability_write = staging_reachability.commit(&mut prune_batch.batch).unwrap();
                    staging_relations.commit(&mut prune_batch.batch).unwrap();
                    drop(reachability_write);
                    drop(statuses_write);
                    drop(reachability_relations_write);
                    drop(level_relations_write);

                    self.flush_prune_batch(&mut prune_batch, &mut metrics);
                    metrics.record_lock_yield(lock_elapsed);
                    if self.is_consensus_exiting.load(Ordering::Relaxed) {
                        drop(prune_guard);
                        info!("Header and Block pruning interrupted: Process is exiting");
                        return;
                    }
                    prune_guard.blocking_yield();
                    lock_acquire_time = Instant::now();
                    reachability_read = Some(self.reachability_store.upgradable_read());
                    metrics.record_lock_reacquire();
                    continue 'staging;
                }
            }

            let reachability_write = staging_reachability.commit(&mut prune_batch.batch).unwrap();
            staging_relations.commit(&mut prune_batch.batch).unwrap();
            drop(reachability_write);
            drop(statuses_write);
            drop(reachability_relations_write);
            drop(level_relations_write);
            break;
        }

        metrics.finalize_lock_hold(lock_acquire_time.elapsed());
        if let Some(guard) = reachability_read.take() {
            drop(guard);
        }
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
            // Set the retention checkpoint to the new retention root only after we successfully pruned its past
            let mut pruning_point_write = self.pruning_point_store.write();
            pruning_point_write.set_retention_checkpoint(&mut prune_batch.batch, retention_period_root).unwrap();
            drop(pruning_point_write);
        }

        self.flush_prune_batch(&mut prune_batch, &mut metrics);

        metrics.set_traversed(traversed, counter);
        metrics.log_summary();
    }

    fn should_flush_prune_batch(&self, batch: &PruneBatch, lock_elapsed: Duration) -> bool {
        if batch.is_empty() {
            return false;
        }

        batch.blocks() >= PRUNE_BATCH_MAX_BLOCKS
            || batch.len() >= PRUNE_BATCH_MAX_OPS
            || batch.size_in_bytes() >= PRUNE_BATCH_MAX_BYTES
            || batch.elapsed() >= Duration::from_millis(PRUNE_BATCH_MAX_DURATION_MS)
            || lock_elapsed >= Duration::from_millis(PRUNE_LOCK_MAX_DURATION_MS)
    }

    fn flush_prune_batch(&self, batch: &mut PruneBatch, metrics: &mut PruningPhaseMetrics) {
        if batch.is_empty() {
            return;
        }

        let ops = batch.len();
        let bytes = batch.size_in_bytes();
        let commit_start = Instant::now();
        let write_batch = batch.take();
        self.db.write(write_batch).unwrap();
        metrics.record_commit("batched", ops, bytes, commit_start.elapsed());
    }

    /// Adjusts the retention period root to latest pruning point sample that covers the retention period.
    /// This is the pruning point sample B such that B.timestamp <= retention_period_days_ago. This may return the old hash if
    /// the retention period cannot be covered yet with the node's current history.
    ///
    /// This function is expected to be called only when a new pruning point is determined and right before
    /// doing any pruning. Pruning point must be the new pruning point this node is advancing to.
    ///
    /// The returned retention_period_root is guaranteed to be in past(pruning_point) or the pruning point itself.
    pub fn advance_retention_period_root(&self, retention_period_root: Hash, pruning_point: Hash) -> Hash {
        match self.config.retention_period_days {
            // If the retention period wasn't set, immediately default to the pruning point.
            None => pruning_point,
            Some(retention_period_days) => {
                // The retention period in milliseconds we need to cover
                // Note: If retention period is set to an amount lower than what the new pruning point would cover
                // this function will simply return the new pruning point. The new pruning point passed as an argument
                // to this function serves as a clamp.
                let retention_period_ms = (retention_period_days * 86400.0 * 1000.0).ceil() as u64;

                // The target timestamp we would like to find a point below
                let sink_timestamp_as_current_time = self.get_sink_timestamp();
                let retention_period_root_ts_target = sink_timestamp_as_current_time.saturating_sub(retention_period_ms);

                // Iterate from the new pruning point to the prev retention root and search for the first point with enough days above it.
                // Note that prev retention root is always a past pruning point, so we can iterate via pruning samples until we reach it.
                let mut new_retention_period_root = pruning_point;

                trace!(
                    "Adjusting the retention period root to cover the required retention period. Target timestamp: {}",
                    retention_period_root_ts_target,
                );

                while new_retention_period_root != retention_period_root {
                    let block = new_retention_period_root;

                    let timestamp = self.headers_store.get_timestamp(block).unwrap();
                    trace!("block | timestamp = {} | {}", block, timestamp);
                    if timestamp <= retention_period_root_ts_target {
                        trace!("block {} timestamp {} >= {}", block, timestamp, retention_period_root_ts_target);
                        // We are now at a pruning point that is at or below our retention period target
                        break;
                    }

                    new_retention_period_root = self.pruning_samples_store.pruning_sample_from_pov(block).unwrap();
                }

                new_retention_period_root
            }
        }
    }

    fn get_sink_timestamp(&self) -> u64 {
        self.headers_store.get_timestamp(self.get_sink()).unwrap()
    }

    fn get_sink(&self) -> Hash {
        self.lkg_virtual_state.load().ghostdag_data.selected_parent
    }

    fn past_pruning_points(&self) -> BlockHashSet {
        (0..self.pruning_point_store.read().pruning_point_index().unwrap())
            .map(|index| self.past_pruning_points_store.get(index).unwrap())
            .collect()
    }

    fn confirm_pruning_depth_below_virtual(&self, pruning_point: Hash) -> bool {
        let virtual_state = self.virtual_stores.read().state.get().unwrap();
        let pp_bs = self.headers_store.get_blue_score(pruning_point).unwrap();
        let pp_daa = self.headers_store.get_daa_score(pruning_point).unwrap();
        virtual_state.ghostdag_data.blue_score >= pp_bs + self.config.params.pruning_depth().get(pp_daa)
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
