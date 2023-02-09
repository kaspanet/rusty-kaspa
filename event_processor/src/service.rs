use std::sync::Arc;

use crate::{processor::EventProcessor, IDENT};
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};

impl AsyncService for EventProcessor {
    fn ident(self: Arc<Self>) -> &'static str {
        IDENT
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("starting {IDENT}");
        let shutdown_finalized_listener = self.shutdown_finalized_listener.clone();
        Box::pin(async move {
            match self.run().await {
                Ok(_) => shutdown_finalized_listener.await,
                Err(err) => panic!("{0} panic! {1}", IDENT, err),
            };
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("signaling {0} exit", IDENT);
        self.signal_shutdown();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("stopping {0}", IDENT);
        let shutdown_finalized_listener = self.shutdown_finalized_listener.clone();
        Box::pin(async move {
            self.signal_shutdown();
            shutdown_finalized_listener.wait();
        })
    }
}
