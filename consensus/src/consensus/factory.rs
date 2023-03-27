use crate::pipeline::ProcessingCounters;

use super::Consensus;
use kaspa_consensus_core::{api::DynConsensus, config::Config};
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use kaspa_consensusmanager::{ConsensusFactory, DynConsensusCtl};
use std::{path::PathBuf, sync::Arc};

pub struct Factory {
    db_root_dir: PathBuf,
    db_parallelism: usize,
    notification_root: Arc<ConsensusNotificationRoot>,
    counters: Arc<ProcessingCounters>,
}

impl Factory {
    pub fn new(
        db_root_dir: PathBuf,
        db_parallelism: usize,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
    ) -> Self {
        Self { db_root_dir, db_parallelism, notification_root, counters }
    }
}

impl ConsensusFactory for Factory {
    fn new_consensus(&self, config: &Config) -> (DynConsensus, DynConsensusCtl) {
        // TODO: manage sub-dirs
        let db = kaspa_database::prelude::open_db(self.db_root_dir.clone(), true, self.db_parallelism);
        let consensus = Arc::new(Consensus::new(db, config, self.notification_root.clone(), self.counters.clone()));
        (consensus.clone(), consensus)
    }
}
