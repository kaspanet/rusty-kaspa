use crate::{
    collector::{ConsensusCollector, ConsensusConverter},
    notification::Notification,
    notifier::ConsensusNotifier,
    root::ConsensusNotificationRoot,
};
use async_channel::Receiver;
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture},
    trace,
};
use kaspa_notify::{
    events::{EventSwitches, EVENT_TYPE_ARRAY},
    subscriber::Subscriber,
};
use kaspa_utils::triggers::SingleTrigger;
use std::sync::Arc;

const NOTIFY_SERVICE: &str = "notify-service";

pub struct NotifyService {
    notifier: Arc<ConsensusNotifier>,
    shutdown: SingleTrigger,
}

impl NotifyService {
    pub fn new(root: Arc<ConsensusNotificationRoot>, notification_receiver: Receiver<Notification>) -> Self {
        let root_events: EventSwitches = EVENT_TYPE_ARRAY[..].into();
        let collector = Arc::new(ConsensusCollector::new(notification_receiver, Arc::new(ConsensusConverter::new())));
        let subscriber = Arc::new(Subscriber::new(root_events, root, 0));
        let notifier = Arc::new(ConsensusNotifier::new(root_events, vec![collector], vec![subscriber], 1, NOTIFY_SERVICE));
        Self { notifier, shutdown: SingleTrigger::default() }
    }

    pub fn notifier(&self) -> Arc<ConsensusNotifier> {
        self.notifier.clone()
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
            match self.notifier.join().await {
                Ok(_) => Ok(()),
                Err(err) => {
                    trace!("Error while stopping {}: {}", NOTIFY_SERVICE, err);
                    Err(AsyncServiceError::Service(err.to_string()))
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
            Ok(())
        })
    }
}
