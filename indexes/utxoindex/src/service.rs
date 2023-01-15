use std::sync::Arc;
use kaspa_core::{service::Service, core::Core};
use std::thread::{JoinHandle, spawn};
use crate::{utxoindex::Signal, processor::Processor};

use super::utxoindex::UtxoIndex;

impl Service for UtxoIndex {
    
    fn ident(self: Arc<UtxoIndex>) -> &'static str { 
        "utxoindex"
    }

    fn start(self: Arc<UtxoIndex>, _core: Arc<Core>) -> Vec<JoinHandle<()>> {
        let jh = spawn( move || { self.run(); () }); //return None for join handle
        vec![jh] //provide vector since that is what kaspa_core wants.
    }

    fn stop(self: Arc<UtxoIndex>) {
        self.signal_send.send(Signal::ShutDown);
    }
}