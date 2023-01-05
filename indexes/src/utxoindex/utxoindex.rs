use std::sync::Arc;

use consensus_core::api::DynConsensus;
use tokio::{sync::mpsc::{Receiver, Sender, channel}, runtime::Runtime};
use super::{stores::store::UtxoIndexStoreManager, ToDo, model::Listeners};
use consensus::model::stores::virtual_state::VirtualState;
use tokio::runtime::Builder;


pub struct UtxoIndex {
        pub consensus: Arc<DynConsensus>,
        pub store: Arc<UtxoIndexStoreManager>,
        pub reciever: Arc<Receiver<VirtualState>>,
        pub listeners: Arc<Listeners>,
        pub runtime: Arc<Runtime>
}

impl UtxoIndex {
    //note: to mimic new from go kaspad need to start with `UtxoIndex::new(consensus: x, num_of_threads: y).reset()` 
    pub fn new(consensus: DynConsensus, num_of_threads: usize) -> Self { 
            let (s,r) = channel::<VirtualState>(usize::MAX); //usize::MAX is same as unbounded, having problems with unbouned channel types. 
            Self { 
                    consensus: consensus,
                    store: todo!(),
                    reciever: Arc::new(r),
                    listeners: todo!(), 
                    runtime: Builder::new_multi_thread()
                    .worker_threads(num_of_threads)
                    .build()
                    .unwrap(),
            };
    }
}
