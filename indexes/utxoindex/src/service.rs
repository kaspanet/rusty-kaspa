use std::sync::Arc;
use kaspa_core::{service::Service, core::Core};
use std::thread::{JoinHandle, spawn};

use crate::processes::process_handler::ProcessHandler;

use super::utxoindex::UtxoIndex;

impl Service for UtxoIndex {
    
    fn ident(self: Arc<UtxoIndex>) -> &'static str { 
        "utxoindex"
    }

    fn start(self: Arc<UtxoIndex>, _core: Arc<Core>) -> Vec<JoinHandle<()>> {
        self.signal_resync_and_process_consensus_events(); //TODO: find correct signal, should it sync and process, or just sync and start later.  
        let jh = spawn(
             move || { tokio::spawn( self.run() ); ()} 
        ); //seems hacky but kaspa_core wants an empty `std::thread` join handle.
        vec![jh] //provide vector since that is what kaspa_core wants.
    }

    fn stop(self: Arc<UtxoIndex>) {
        self.signal_shutdown();
    }
}