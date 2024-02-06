use crate::{processor::Processor, IDENT};
use kaspa_consensus_notify::{
    connection::ConsensusChannelConnection, notification::Notification as ConsensusNotification, notifier::ConsensusNotifier,
};
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture},
    trace, warn,
};

use kaspa_index_core::notifier::IndexNotifier;
use kaspa_notify::{
    connection::ChannelType,
    events::EventType,
    scope::{Scope, UtxosChangedScope, VirtualChainChangedScope},
};
use kaspa_confindex::api::ConfIndexProxy;
use kaspa_utils::{channel::Channel, triggers::SingleTrigger};
use kaspa_utxoindex::api::UtxoIndexProxy;
use std::{collections::HashSet, sync::Arc};

const INDEX_SERVICE: &str = IDENT;

pub struct IndexService {
    utxoindex: Option<UtxoIndexProxy>,
    confindex: Option<ConfIndexProxy>,
    notifier: Arc<IndexNotifier>,
    shutdown: SingleTrigger,
}

impl IndexService {
    pub fn new(
        consensus_notifier: &Arc<ConsensusNotifier>,
        utxoindex: Option<UtxoIndexProxy>,
        confindex: Option<ConfIndexProxy>,
    ) -> Self {
        // Prepare consensus-notify objects
        let consensus_notify_channel = Channel::<ConsensusNotification>::default();
        let consensus_notify_listener_id = consensus_notifier
            .register_new_listener(ConsensusChannelConnection::new(consensus_notify_channel.sender(), ChannelType::Closable));

        // Prepare the index-processor notifier
        // No subscriber is defined here because the subscription are manually created during the construction and never changed after that.
        let mut event_types = HashSet::<EventType>::new();

        if utxoindex.is_some() {
            event_types.insert(EventType::UtxosChanged);
            event_types.insert(EventType::PruningPointUtxoSetOverride);
        }
        if confindex.is_some() {
            event_types.insert(EventType::VirtualChainChanged);
            event_types.insert(EventType::ChainAcceptanceDataPruned);
        }

        let events = event_types.iter().cloned().collect::<Vec<EventType>>().as_slice().into();

        let collector = Arc::new(Processor::new(utxoindex.clone(), confindex.clone(), consensus_notify_channel.receiver()));
        let notifier = Arc::new(IndexNotifier::new(INDEX_SERVICE, events, vec![collector], vec![], 1));

        // Manually subscribe to index-processor related event types
        for event in event_types.into_iter() {
            let scope = match event {
                EventType::UtxosChanged => Scope::UtxosChanged(UtxosChangedScope::default()),
                EventType::VirtualChainChanged => Scope::VirtualChainChanged(VirtualChainChangedScope::new(true)),
                _ => Scope::from(event),
            };

            consensus_notifier.try_start_notify(consensus_notify_listener_id, scope).expect("the subscription always succeeds");
        }

        Self { utxoindex, confindex, notifier, shutdown: SingleTrigger::default() }
    }

    pub fn notifier(&self) -> Arc<IndexNotifier> {
        self.notifier.clone()
    }

    pub fn utxoindex(&self) -> Option<UtxoIndexProxy> {
        self.utxoindex.clone()
    }

    pub fn confindex(&self) -> Option<ConfIndexProxy> {
        self.confindex.clone()
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
            match self.notifier.join().await {
                Ok(_) => Ok(()),
                Err(err) => {
                    warn!("Error while stopping {}: {}", INDEX_SERVICE, err);
                    Err(AsyncServiceError::Service(err.to_string()))
                }
            }
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", INDEX_SERVICE);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", INDEX_SERVICE);
            Ok(())
        })
    }
}
