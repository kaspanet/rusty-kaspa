//! TODO: module comment about locking safety and consistency of various pruning stores

use crate::{
    model::stores::{
        ghostdag::{CompactGhostdagData, DbGhostdagStore},
        headers::DbHeadersStore,
        past_pruning_points::DbPastPruningPointsStore,
        pruning::{DbPruningStore, PruningStore, PruningStoreReader},
        reachability::DbReachabilityStore,
    },
    processes::pruning::PruningManager,
};
use crossbeam_channel::Receiver as CrossbeamReceiver;
use kaspa_database::prelude::DB;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use rocksdb::WriteBatch;
use std::sync::Arc;

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

    // Stores
    pruning_store: Arc<RwLock<DbPruningStore>>,
    past_pruning_points_store: Arc<DbPastPruningPointsStore>,

    // Managers
    pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
}

impl PruningProcessor {
    pub fn new(
        receiver: CrossbeamReceiver<PruningProcessingMessage>,
        db: Arc<DB>,
        pruning_store: Arc<RwLock<DbPruningStore>>,
        past_pruning_points_store: Arc<DbPastPruningPointsStore>,
        pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
    ) -> Self {
        Self { receiver, db, pruning_store, past_pruning_points_store, pruning_manager }
    }

    pub fn worker(self: &Arc<Self>) {
        while let Ok(msg) = self.receiver.recv() {
            match msg {
                PruningProcessingMessage::Exit => break,
                PruningProcessingMessage::Process { sink_ghostdag_data } => {
                    self.advance_pruning_point_and_candidate_if_possible(sink_ghostdag_data);
                }
            };
        }
    }

    fn advance_pruning_point_and_candidate_if_possible(&self, sink_ghostdag_data: CompactGhostdagData) {
        let pruning_read_guard = self.pruning_store.upgradable_read();
        let current_pruning_info = pruning_read_guard.get().unwrap();
        let (new_pruning_points, new_candidate) = self.pruning_manager.next_pruning_points_and_candidate_by_ghostdag_data(
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
            write_guard.set_batch(&mut batch, *new_pruning_points.last().unwrap(), new_candidate, new_pp_index).unwrap();
            self.db.write(batch).unwrap();
            // TODO: Move PP UTXO etc
        } else if new_candidate != current_pruning_info.candidate {
            let mut write_guard = RwLockUpgradableReadGuard::upgrade(pruning_read_guard);
            write_guard.set(current_pruning_info.pruning_point, new_candidate, current_pruning_info.index).unwrap();
        }
    }
}
