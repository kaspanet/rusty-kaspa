use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_core::{core::Core, service::Service};
use std::sync::Arc;

use super::utxoindex::UtxoIndex;

const UTXOINDEX: &str = "utxoindex";

impl AsyncService for UtxoIndex {
    fn ident(self: Arc<UtxoIndex>) -> &'static str {
        UTXOINDEX
    }

    fn start(self: Arc<UtxoIndex>) -> AsyncServiceFuture {
        trace!("starting {}", UTXOINDEX);
        let shutdown_sender = self.shutdown_sender.clone();
        runner = self.run();
        Box::Pin(async move {
            self.shutdown_listener.wait()
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("signaling {} exit", UTXOINDEX);
        self.shutdown_sender.send(());
    }

    fn stop(self: Arc<UtxoIndex>) -> AsyncServiceFuture {
        trace!("stopping {0}", UTXOINDEX);
        Box::pin(async move {
            self.shutdown_trigger.trigger();
            self.shutdown_listener.wait();
        })
    }
}
