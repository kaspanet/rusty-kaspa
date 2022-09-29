use crate::{
    errors::BlockProcessResult,
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagStoreReader},
            headers::DbHeadersStore,
            past_pruning_points::DbPastPruningPointsStore,
            pruning::{DbPruningStore, PruningStore, PruningStoreReader},
            reachability::DbReachabilityStore,
            statuses::{BlockStatus, DbStatusesStore},
            DB,
        },
    },
    pipeline::deps_manager::BlockTask,
    processes::pruning::PruningManager,
};
use consensus_core::{block::Block, blockhash::VIRTUAL};
use crossbeam_channel::Receiver;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use rocksdb::WriteBatch;
use std::sync::{
    atomic::{self, AtomicBool},
    Arc,
};

pub struct VirtualStateProcessor {
    // Channels
    receiver: Receiver<BlockTask>,

    // DB
    db: Arc<DB>,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pruning_store: Arc<RwLock<DbPruningStore>>,
    past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    ghostdag_store: Arc<DbGhostdagStore>,

    // Managers and services
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,

    is_updating_pruning_point_or_candidate: AtomicBool,
}

impl VirtualStateProcessor {
    pub fn new(
        receiver: Receiver<BlockTask>,
        db: Arc<DB>,
        statuses_store: Arc<RwLock<DbStatusesStore>>,
        pruning_store: Arc<RwLock<DbPruningStore>>,
        ghostdag_store: Arc<DbGhostdagStore>,
        past_pruning_points_store: Arc<DbPastPruningPointsStore>,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
    ) -> Self {
        Self {
            receiver,
            db,

            statuses_store,
            reachability_service,
            is_updating_pruning_point_or_candidate: false.into(),
            pruning_store,
            ghostdag_store,
            past_pruning_points_store,

            pruning_manager,
        }
    }

    pub fn worker(self: &Arc<VirtualStateProcessor>) {
        while let Ok(task) = self.receiver.recv() {
            match task {
                BlockTask::Exit => break,
                BlockTask::Process(block, result_transmitters) => {
                    let res = self.resolve_virtual(&block);
                    for transmitter in result_transmitters {
                        // We don't care if receivers were dropped
                        let _ = transmitter.send(res.clone());
                    }
                }
            };
        }
    }

    fn resolve_virtual(self: &Arc<VirtualStateProcessor>, block: &Block) -> BlockProcessResult<BlockStatus> {
        Ok(BlockStatus::StatusUTXOPendingVerification)
    }

    fn maybe_update_pruning_point_and_candidate(self: &Arc<Self>) {
        if let Err(_) = self.is_updating_pruning_point_or_candidate.compare_exchange(
            false,
            true,
            atomic::Ordering::Acquire,
            atomic::Ordering::Relaxed,
        ) {
            return;
        }

        {
            let pruning_read_guard = self.pruning_store.upgradable_read();
            let current_pp = pruning_read_guard.pruning_point().unwrap();
            let current_pp_candidate = pruning_read_guard.pruning_point_candidate().unwrap();
            let virtual_gd = self.ghostdag_store.get_compact_data(VIRTUAL).unwrap();
            let (new_pruning_point, new_candidate) = self.pruning_manager.next_pruning_point_and_candidate_by_block_hash(
                virtual_gd,
                None,
                current_pp_candidate,
                current_pp,
            );

            if new_pruning_point != current_pp {
                let mut batch = WriteBatch::default();
                let new_pp_index = pruning_read_guard.pruning_point_index().unwrap() + 1;
                let mut write_guard = RwLockUpgradableReadGuard::upgrade(pruning_read_guard);
                write_guard.set_batch(&mut batch, new_pruning_point, new_candidate, new_pp_index).unwrap();
                self.past_pruning_points_store.insert_batch(&mut batch, new_pp_index, new_pruning_point).unwrap();
                self.db.write(batch).unwrap();
                // TODO: Move PP UTXO etc
            } else if new_candidate != current_pp_candidate {
                let pp_index = pruning_read_guard.pruning_point_index().unwrap();
                let mut write_guard = RwLockUpgradableReadGuard::upgrade(pruning_read_guard);
                write_guard.set(new_pruning_point, new_candidate, pp_index).unwrap();
            }
        }

        self.is_updating_pruning_point_or_candidate.store(false, atomic::Ordering::Release);
    }
}
