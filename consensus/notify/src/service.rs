use crate::notifier::ConsensusNotifier;
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};
use kaspa_utils::triggers::SingleTrigger;
use std::sync::Arc;

const NOTIFY_SERVICE: &str = "notify-service";

pub struct NotifyService {
    notifier: Arc<ConsensusNotifier>,
    shutdown: SingleTrigger,
}

impl NotifyService {
    pub fn new(notifier: Arc<ConsensusNotifier>) -> Self {
        Self { notifier, shutdown: SingleTrigger::default() }
    }
}

impl AsyncService for NotifyService {
    fn ident(self: Arc<Self>) -> &'static str {
        NOTIFY_SERVICE
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} starting", NOTIFY_SERVICE);

        // Prepare a shutdown signal receiver
        let shutdown_signal = self.shutdown.listener.clone();

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            self.notifier.clone().start();

            // Keep the notifier running until a service shutdown signal is received
            shutdown_signal.await;
            match self.notifier.stop().await {
                Ok(_) => {}
                Err(err) => {
                    trace!("Error while stopping {}: {}", NOTIFY_SERVICE, err);
                }
            }
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", NOTIFY_SERVICE);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} stopping", NOTIFY_SERVICE);
        Box::pin(async move {
            trace!("{} exiting", NOTIFY_SERVICE);
        })
    }
}
