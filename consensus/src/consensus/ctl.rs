use super::{factory::MultiConsensusManagementStore, Consensus};
use kaspa_consensusmanager::ConsensusCtl;
use kaspa_database::prelude::DB;
use parking_lot::RwLock;
use std::{
    path::PathBuf,
    sync::{Arc, Weak},
    thread::JoinHandle,
};

pub struct Ctl {
    management_store: Arc<RwLock<MultiConsensusManagementStore>>,
    consensus_db_ref: Weak<DB>,
    consensus_db_path: PathBuf,
    consensus: Arc<Consensus>,
}

impl Ctl {
    pub fn new(
        management_store: Arc<RwLock<MultiConsensusManagementStore>>,
        consensus_db: Arc<DB>,
        consensus: Arc<Consensus>,
    ) -> Self {
        let consensus_db_path = consensus_db.path().to_owned();
        let consensus_db_ref = Arc::downgrade(&consensus_db);
        Self { management_store, consensus_db_ref, consensus_db_path, consensus }
    }
}

impl ConsensusCtl for Ctl {
    fn start(&self) -> Vec<JoinHandle<()>> {
        self.consensus.run_processors()
    }

    fn stop(&self) {
        self.consensus.signal_exit()
    }

    fn make_active(&self) {
        // TODO: pass a value to make sure the correct consensus is committed
        self.management_store.write().commit_staging_consensus().unwrap();

        // TODO: delete previous active
    }

    fn delete(&self) {
        // TODO: see above
        self.management_store.write().cancel_staging_consensus().unwrap();

        // TODO: delete staging
        // for _ in 0..16 {
        //     if self.consensus_db_ref.strong_count() > 0 {
        //         // Sometimes another thread is shuting-down and cleaning resources
        //         std::thread::sleep(std::time::Duration::from_millis(500));
        //     } else {
        //         break;
        //     }
        // }
        // assert_eq!(self.consensus_db_ref.strong_count(), 0, "DB has strong references and cannot be deleted");
        // let options = rocksdb::Options::default();
        // DB::destroy(&options, self.consensus_db_path.clone())
        //     .expect("DB is expected to be deletable since there are no references to it");
    }
}

/// Impl for test purposes
impl ConsensusCtl for Consensus {
    fn start(&self) -> Vec<JoinHandle<()>> {
        self.run_processors()
    }

    fn stop(&self) {
        self.signal_exit()
    }

    fn make_active(&self) {
        unimplemented!()
    }

    fn delete(&self) {
        unimplemented!()
    }
}
