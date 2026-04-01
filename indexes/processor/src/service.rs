use crate::{IDENT, processor::Processor};
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
    listener::ListenerLifespan,
    scope::{
        BlockAddedScope, PruningPointUtxoSetOverrideScope, RetentionRootChangedScope, UtxosChangedScope, VirtualChainChangedScope,
    },
    subscription::{MutationPolicies, UtxosChangedMutationPolicy, context::SubscriptionContext},
};
use kaspa_txindex::api::TxIndexProxy;
use kaspa_utils::{channel::Channel, triggers::SingleTrigger};
use kaspa_utxoindex::api::UtxoIndexProxy;
use std::sync::Arc;

const INDEX_SERVICE: &str = IDENT;

pub struct IndexService {
    utxoindex: Option<UtxoIndexProxy>,
    txindex: Option<TxIndexProxy>,
    notifier: Arc<IndexNotifier>,
    shutdown: SingleTrigger,
}

impl IndexService {
    pub fn new(
        consensus_notifier: &Arc<ConsensusNotifier>,
        subscription_context: SubscriptionContext,
        utxoindex: Option<UtxoIndexProxy>,
        txindex: Option<TxIndexProxy>,
    ) -> Self {
        // This notifier UTXOs subscription granularity to consensus notifier
        let policies = MutationPolicies::new(UtxosChangedMutationPolicy::Wildcard);

        // Prepare consensus-notify objects
        let consensus_notify_channel = Channel::<ConsensusNotification>::default();
        let consensus_notify_listener_id = consensus_notifier.register_new_listener(
            ConsensusChannelConnection::new(INDEX_SERVICE, consensus_notify_channel.sender(), ChannelType::Closable),
            ListenerLifespan::Static(policies),
        );

        // Prepare the index-processor notifier
        // No subscriber is defined here because the subscription are manually created during the construction and never changed after that.
        let events = match (utxoindex.is_some(), txindex.is_some()) {
            (false, false) => [].as_ref().into(),
            (true, false) => [EventType::UtxosChanged, EventType::PruningPointUtxoSetOverride].as_ref().into(),
            (false, true) => [EventType::VirtualChainChanged, EventType::BlockAdded, EventType::RetentionRootChanged].as_ref().into(),
            (true, true) => [
                EventType::UtxosChanged,
                EventType::PruningPointUtxoSetOverride,
                EventType::VirtualChainChanged,
                EventType::BlockAdded,
                EventType::RetentionRootChanged,
            ]
            .as_ref()
            .into(),
        };

        let collector = Arc::new(Processor::new(utxoindex.clone(), txindex.clone(), consensus_notify_channel.receiver()));
        let notifier = Arc::new(IndexNotifier::new(INDEX_SERVICE, events, vec![collector], vec![], subscription_context, 1, policies));

        // Manually subscribe to index-processor related event types
        if utxoindex.is_some() {
            consensus_notifier
                .try_start_notify(consensus_notify_listener_id, UtxosChangedScope::default().into())
                .expect("the subscription always succeeds");
            consensus_notifier
                .try_start_notify(consensus_notify_listener_id, PruningPointUtxoSetOverrideScope::default().into())
                .expect("the subscription always succeeds");
        };
        if txindex.is_some() {
            consensus_notifier
                .try_start_notify(consensus_notify_listener_id, VirtualChainChangedScope::new(true, true, true).into())
                .expect("the subscription always succeeds");
            consensus_notifier
                .try_start_notify(consensus_notify_listener_id, BlockAddedScope::default().into())
                .expect("the subscription always succeeds");
            consensus_notifier
                .try_start_notify(consensus_notify_listener_id, RetentionRootChangedScope::default().into())
                .expect("the subscription always succeeds");
        };

        Self { utxoindex, txindex, notifier, shutdown: SingleTrigger::default() }
    }

    pub fn notifier(&self) -> Arc<IndexNotifier> {
        self.notifier.clone()
    }

    pub fn utxoindex(&self) -> Option<UtxoIndexProxy> {
        self.utxoindex.clone()
    }

    pub fn txindex(&self) -> Option<TxIndexProxy> {
        self.txindex.clone()
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
