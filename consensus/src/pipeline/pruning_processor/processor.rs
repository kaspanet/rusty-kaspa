//! TODO: module comment about locking safety and consistency of various pruning stores

use crate::{
    consensus::storage::ConsensusStorage,
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            ghostdag::{CompactGhostdagData, DbGhostdagStore},
            headers::{DbHeadersStore, HeaderStoreReader},
            past_pruning_points::DbPastPruningPointsStore,
            pruning::{PruningStore, PruningStoreReader},
            reachability::DbReachabilityStore,
            utxo_diffs::UtxoDiffsStoreReader,
            utxo_set::UtxoSetStore,
        },
    },
    processes::pruning::PruningManager,
};
use crossbeam_channel::Receiver as CrossbeamReceiver;
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_core::info;
use kaspa_database::prelude::DB;
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use parking_lot::RwLockUpgradableReadGuard;
use rocksdb::WriteBatch;
use std::{ops::Deref, sync::Arc};
use tokio::sync::RwLock as TokioRwLock;

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
    pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
    reachability_service: MTReachabilityService<DbReachabilityStore>,

    // Pruning lock
    pruning_lock: Arc<TokioRwLock<()>>,
}

impl Deref for PruningProcessor {
    type Target = ConsensusStorage;

    fn deref(&self) -> &Self::Target {
        &self.storage
    }
}

impl PruningProcessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: CrossbeamReceiver<PruningProcessingMessage>,
        db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        pruning_lock: Arc<TokioRwLock<()>>,
    ) -> Self {
        Self { receiver, db, storage: storage.clone(), pruning_manager, reachability_service, pruning_lock }
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

            let _prune_guard = self.pruning_lock.blocking_write();
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
}
