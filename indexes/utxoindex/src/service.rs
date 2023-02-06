use std::sync::Arc;

use kaspa_core::{
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};

use crate::UtxoIndex;

const UTXOINDEX: &str = "utxoindex";

impl AsyncService for UtxoIndex {
    fn ident(self: Arc<UtxoIndex>) -> &'static str {
        UTXOINDEX
    }

    fn start(self: Arc<UtxoIndex>) -> AsyncServiceFuture {
        trace!("starting {UTXOINDEX}");
        let shutdown_finalized_listener = self.shutdown_finalized_listener.clone();
        Box::pin(async move {
            match self.maybe_reset() {
                Ok(_) => (),
                Err(err) => panic!("could not start utxoindex: {err}"),
            }
            self.process_events().await;
            shutdown_finalized_listener.await;
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("signaling {} exit", UTXOINDEX);
        self.signal_shutdown();
    }

    fn stop(self: Arc<UtxoIndex>) -> AsyncServiceFuture {
        trace!("stopping {0}", UTXOINDEX);
        let shutdown_finalized_listener = self.shutdown_finalized_listener.clone();
        Box::pin(async move {
            self.signal_shutdown();
            shutdown_finalized_listener.wait();
        })
    }
}
