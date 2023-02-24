use crate::{notifier::IndexNotifier, processor::Processor, IDENT};
use consensus_notify::{
    connection::ConsensusChannelConnection, notification::Notification as ConsensusNotification, service::NotifyService,
};
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};
use kaspa_notify::{
    events::{EventSwitches, EventType},
    scope::{PruningPointUtxoSetOverrideScope, Scope, UtxosChangedScope},
};
use kaspa_utils::{channel::Channel, triggers::SingleTrigger};
use std::sync::Arc;
use utxoindex::api::DynUtxoIndexApi;

const INDEX_SERVICE: &str = IDENT;

pub struct IndexService {
    utxoindex: DynUtxoIndexApi,
    notifier: Arc<IndexNotifier>,
    shutdown: SingleTrigger,
}

impl IndexService {
    pub fn new(notify_service: Arc<NotifyService>, utxoindex: DynUtxoIndexApi) -> Self {
        // Prepare consensus-notify objects
        let consensus_notify_channel = Channel::<ConsensusNotification>::default();
        let consensus_notify_listener_id =
            notify_service.notifier().register_new_listener(ConsensusChannelConnection::new(consensus_notify_channel.sender()));

        // Prepare the index-processor notifier
        // No subscriber is defined here because the subscription are manually created during the construction and never changed after that.
        let events: EventSwitches = [EventType::UtxosChanged, EventType::PruningPointUtxoSetOverride].as_ref().into();
        let collector = Arc::new(Processor::new(utxoindex.clone(), consensus_notify_channel.receiver()));
        let notifier = Arc::new(IndexNotifier::new(events, vec![collector], vec![], 1, INDEX_SERVICE));

        // Manually subscribe to index-processor related event types
        notify_service
            .notifier()
            .try_start_notify(consensus_notify_listener_id, Scope::UtxosChanged(UtxosChangedScope::default()))
            .expect("the subscription always succeeds");
        notify_service
            .notifier()
            .try_start_notify(consensus_notify_listener_id, Scope::PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideScope {}))
            .expect("the subscription always succeeds");

        Self { utxoindex, notifier, shutdown: SingleTrigger::default() }
    }

    pub fn notifier(&self) -> Arc<IndexNotifier> {
        self.notifier.clone()
    }

    pub fn utxoindex(&self) -> DynUtxoIndexApi {
        self.utxoindex.clone()
    }
}

impl AsyncService for IndexService {
    fn ident(self: Arc<Self>) -> &'static str {
        INDEX_SERVICE
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} starting", INDEX_SERVICE);

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
                    trace!("Error while stopping {}: {}", INDEX_SERVICE, err);
                }
            }
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", INDEX_SERVICE);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} stopping", INDEX_SERVICE);
        Box::pin(async move {
            trace!("{} exiting", INDEX_SERVICE);
        })
    }
}
