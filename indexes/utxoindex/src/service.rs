use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use log::trace;
use std::sync::Arc;

use super::utxoindex::UtxoIndex;

const UTXOINDEX: &str = "utxoindex";

impl AsyncService for UtxoIndex {
    fn ident(self: Arc<UtxoIndex>) -> &'static str {
        UTXOINDEX
    }

    fn start(self: Arc<UtxoIndex>) -> AsyncServiceFuture {
        trace!("starting {}", UTXOINDEX);
        let shutdown_listener = self.shutdown_listener.clone();
        Box::pin(async move {
            self.run().await.expect("expected utxo start");
            shutdown_listener.wait();
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("signaling {} exit", UTXOINDEX);
        self.shutdown_trigger.trigger();
    }

    fn stop(self: Arc<UtxoIndex>) -> AsyncServiceFuture {
        trace!("stopping {0}", UTXOINDEX);
        Box::pin(async move {
            self.shutdown_trigger.trigger();
            self.shutdown_listener.wait();
        })
    }
}
