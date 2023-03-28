use super::Consensus;
use crate::{model::stores::U64Key, pipeline::ProcessingCounters};
use kaspa_consensus_core::config::Config;
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use kaspa_consensusmanager::{ConsensusFactory, ConsensusInstance, DynConsensusCtl};
use kaspa_database::prelude::{CachedDbAccess, CachedDbItem, DB};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::RwLock as TokioRwLock;

#[derive(Serialize, Deserialize, Clone)]
pub struct ConsensusEntry {
    directory_name: String,
    creation_timestamp: u64,
    // TODO: add additional metadata
}

const MULTI_CONSENSUS_PREFIX: &[u8] = b"multi-consensus-prefix";
const CURRENT_CONSENSUS_KEY: &[u8] = b"current-consensus-key";

#[derive(Clone)]
pub struct MultiConsensusManagementStore {
    db: Arc<DB>,
    access: CachedDbAccess<U64Key, ConsensusEntry>,
    current_consensus: CachedDbItem<u64>,
}

impl MultiConsensusManagementStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            db: db.clone(),
            access: CachedDbAccess::new(db.clone(), 0, MULTI_CONSENSUS_PREFIX.to_vec()),
            current_consensus: CachedDbItem::new(db, CURRENT_CONSENSUS_KEY.to_vec()),
        }
    }
}

pub struct Factory {
    config: Config,
    db_root_dir: PathBuf,
    db_parallelism: usize,
    notification_root: Arc<ConsensusNotificationRoot>,
    counters: Arc<ProcessingCounters>,
}

impl Factory {
    pub fn new(
        config: &Config,
        db_root_dir: PathBuf,
        db_parallelism: usize,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
    ) -> Self {
        let mut config = config.clone();
        config.process_genesis = true;
        Self { config, db_root_dir, db_parallelism, notification_root, counters }
    }
}

impl ConsensusFactory for Factory {
    fn new_active_consensus(&self) -> (ConsensusInstance, DynConsensusCtl) {
        // TODO: manage sub-dirs
        let db = kaspa_database::prelude::open_db(self.db_root_dir.clone(), true, self.db_parallelism);
        // TODO: pass to consensus
        let session_lock = Arc::new(TokioRwLock::new(()));
        let consensus = Arc::new(Consensus::new(db, &self.config, self.notification_root.clone(), self.counters.clone()));
        (ConsensusInstance::new(session_lock, consensus.clone()), consensus)
    }

    fn new_staging_consensus(&self) -> (ConsensusInstance, DynConsensusCtl) {
        todo!()
    }
}
