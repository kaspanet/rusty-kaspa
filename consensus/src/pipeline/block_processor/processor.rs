use crate::{
    errors::BlockProcessResult,
    model::{
        services::reachability::MTReachabilityService,
        stores::{reachability::DbReachabilityStore, statuses::DbStatusesStore, DB},
    },
    pipeline::deps_manager::{BlockTask, BlockTaskDependencyManager},
};
use consensus_core::block::Block;
use crossbeam_channel::{Receiver, Sender};
use hashes::Hash;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct BlockBodyProcessor {
    // Channels
    receiver: Receiver<BlockTask>,
    sender: Sender<BlockTask>,

    // DB
    db: Arc<DB>,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,

    // Managers and services
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,

    // Dependency manager
    task_manager: BlockTaskDependencyManager,
}

impl BlockBodyProcessor {
    pub fn new(
        receiver: Receiver<BlockTask>, sender: Sender<BlockTask>, db: Arc<DB>,
        statuses_store: Arc<RwLock<DbStatusesStore>>, reachability_service: MTReachabilityService<DbReachabilityStore>,
    ) -> Self {
        Self {
            receiver,
            sender,
            db,
            statuses_store,
            reachability_service,
            task_manager: BlockTaskDependencyManager::new(),
        }
    }

    pub fn worker(self: &Arc<BlockBodyProcessor>) {
        while let Ok(task) = self.receiver.recv() {
            match task {
                BlockTask::Exit => break,
                BlockTask::Process(block, result_transmitters) => {
                    let hash = block.header.hash;
                    if self
                        .task_manager
                        .register(block, result_transmitters)
                    {
                        let processor = self.clone();
                        rayon::spawn(move || {
                            processor.queue_block(hash);
                        });
                    }
                }
            };
        }

        // Wait until all workers are idle before exiting
        self.task_manager.wait_for_idle();

        // Pass the exit signal on to the following processor
        self.sender.send(BlockTask::Exit).unwrap();
    }

    fn queue_block(self: &Arc<BlockBodyProcessor>, hash: Hash) {
        if let Some(block) = self.task_manager.try_begin(hash) {
            let res = self.process_block_body(&block);

            let dependent_tasks = self
                .task_manager
                .end(hash, |block, result_transmitters| {
                    if res.is_err() {
                        for transmitter in result_transmitters {
                            // We don't care if receivers were dropped
                            let _ = transmitter.send(res.clone());
                        }
                    } else {
                        self.sender
                            .send(BlockTask::Process(block, result_transmitters))
                            .unwrap();
                    }
                });

            for dep in dependent_tasks {
                let processor = self.clone();
                rayon::spawn(move || processor.queue_block(dep));
            }
        }
    }

    fn process_block_body(self: &Arc<BlockBodyProcessor>, block: &Block) -> BlockProcessResult<()> {
        Self::validate_body_in_isolation(block)
    }
}
