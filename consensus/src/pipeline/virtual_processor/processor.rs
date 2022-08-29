use crate::{
    errors::BlockProcessResult,
    model::{
        services::reachability::MTReachabilityService,
        stores::{reachability::DbReachabilityStore, statuses::DbStatusesStore, DB},
    },
    pipeline::deps_manager::BlockTask,
};
use consensus_core::block::Block;
use crossbeam_channel::Receiver;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct VirtualStateProcessor {
    // Channels
    receiver: Receiver<BlockTask>,

    // DB
    db: Arc<DB>,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,

    // Managers and services
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
}

impl VirtualStateProcessor {
    pub fn new(
        receiver: Receiver<BlockTask>, db: Arc<DB>, statuses_store: Arc<RwLock<DbStatusesStore>>,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
    ) -> Self {
        Self { receiver, db, statuses_store, reachability_service }
    }

    pub fn worker(self: &Arc<VirtualStateProcessor>) {
        while let Ok(task) = self.receiver.recv() {
            match task {
                BlockTask::Exit => break,
                BlockTask::Process(block, result_transmitters) => {
                    let res = self.resolve_virtual(&block);
                    for tx in result_transmitters {
                        tx.send(res.clone()).unwrap();
                    }
                }
            };
        }
    }

    fn resolve_virtual(self: &Arc<VirtualStateProcessor>, block: &Block) -> BlockProcessResult<()> {
        Ok(())
    }
}
