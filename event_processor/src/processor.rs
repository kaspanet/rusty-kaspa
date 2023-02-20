use crate::{
    errors::EventProcessorResult,
    notify::{
        BlockAddedNotification, FinalityConflictNotification, FinalityConflictResolvedNotification, NewBlockTemplateNotification,
        Notification, PruningPointUTXOSetOverrideNotification, UtxosChangedNotification, VirtualDaaScoreChangedNotification,
        VirtualSelectedParentBlueScoreChangedNotification, VirtualSelectedParentChainChangedNotification,
    },
    IDENT,
};
use async_channel::{Receiver, Sender};
use consensus_core::events::{
    BlockAddedEvent, ConsensusEvent, FinalityConflictEvent, FinalityConflictResolvedEvent, NewBlockTemplateEvent,
    PruningPointUTXOSetOverrideEvent, VirtualChangeSetEvent,
};
use futures::{select, FutureExt};
use kaspa_core::trace;
use std::sync::Arc;
use triggered::{Listener, Trigger};
use utxoindex::api::DynUtxoIndexApi;
use utxoindex::events::UtxoIndexEvent;

/// The [`EventProcessor`] takes in events from kaspad and processes these to [`Notification`]s,
/// It also feeds and controls indexers, thereby extracting indexed events.
/// [`Notification`]s are in a rpc-core friendly format.  
#[derive(Clone)]
pub struct EventProcessor {
    utxoindex: DynUtxoIndexApi,

    rpc_send: Sender<Notification>,
    consensus_recv: Receiver<ConsensusEvent>,

    shutdown_trigger: Trigger,
    shutdown_listener: Listener,

    shutdown_finalized_trigger: Trigger,
    pub shutdown_finalized_listener: Listener,
}

impl EventProcessor {
    pub fn new(utxoindex: DynUtxoIndexApi, consensus_recv: Receiver<ConsensusEvent>, rpc_send: Sender<Notification>) -> Self {
        let (shutdown_trigger, shutdown_listener) = triggered::trigger();
        let (shutdown_finalized_trigger, shutdown_finalized_listener) = triggered::trigger();

        Self {
            utxoindex,

            rpc_send,
            consensus_recv,

            shutdown_trigger,
            shutdown_listener,
            shutdown_finalized_trigger,
            shutdown_finalized_listener,
        }
    }

    /// Processes the [`ConsensusEvent`] [`NewBlockTemplateEvent`] to the [`Notification`] [`NewBlockTemplateNotification`]
    /// and sends this through the [`EventProcessor`] channel.
    async fn process_new_block_template_event(&self) -> EventProcessorResult<()> {
        trace!("[{IDENT}]: processing {:?}", NewBlockTemplateEvent {});
        self.rpc_send.send(Notification::NewBlockTemplate(NewBlockTemplateNotification {})).await?;
        Ok(())
    }

    /// Processes the [`ConsensusEvent`] [`Arc<BlockAddedEvent>`] to the [`Notification`] [`Arc<BlockAddedNotification>`]
    /// and sends this through the [`EventProcessor`] channel.
    async fn process_block_added_event(&self, block_added_event: Arc<BlockAddedEvent>) -> EventProcessorResult<()> {
        trace!("processing {:?}", block_added_event);
        self.rpc_send
            .send(Notification::BlockAdded(Arc::new(BlockAddedNotification { block: block_added_event.block.clone() })))
            .await?;
        Ok(())
    }

    /// Processes the [`ConsensusEvent`] [`Arc<VirtualChangeSetEvent>`] to the [`Notification`]s:
    /// [`VirtualSelectedParentBlueScoreChangedNotification`], [`VirtualDaaScoreChangedNotification`],
    /// [`VirtualSelectedParentChainChangedNotification`] and potentially triggers an update on the [`UtxoIndex`]
    /// with a resulting [`UtxosChangedNotification`] (if the utxoindex is active),
    /// and sends these through the [`EventProcessor`] channel.
    async fn process_virtual_state_change_set_event(
        &self,
        virtual_change_set_event: Arc<VirtualChangeSetEvent>,
    ) -> EventProcessorResult<()> {
        trace!("[{IDENT}]: processing {:?}", virtual_change_set_event);
        if let Some(utxoindex) = self.utxoindex.as_deref() {
            let UtxoIndexEvent::UtxosChanged(utxo_changed_event) = utxoindex
                .write()
                .update(virtual_change_set_event.accumulated_utxo_diff.clone(), virtual_change_set_event.parents.clone())?;
            self.rpc_send
                .send(Notification::UtxosChanged(Arc::new(UtxosChangedNotification {
                    added: utxo_changed_event.added.clone(),
                    removed: utxo_changed_event.removed.clone(),
                })))
                .await?;
        };

        self.rpc_send
            .send(Notification::VirtualSelectedParentBlueScoreChanged(VirtualSelectedParentBlueScoreChangedNotification {
                virtual_selected_parent_blue_score: virtual_change_set_event.selected_parent_blue_score,
            }))
            .await?;

        self.rpc_send
            .send(Notification::VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification {
                virtual_daa_score: virtual_change_set_event.daa_score,
            }))
            .await?;

        self.rpc_send
            .send(Notification::VirtualSelectedParentChainChanged(Arc::new(VirtualSelectedParentChainChangedNotification {
                added_chain_block_hashes: virtual_change_set_event.mergeset_blues.clone(),
                removed_chain_block_hashes: virtual_change_set_event.mergeset_reds.clone(),
                accepted_transaction_ids: virtual_change_set_event.accepted_tx_ids.clone(),
            })))
            .await?;
        Ok(())
    }

    /// Processes the [`ConsensusEvent`] [`PruningPointUTXOSetOverrideEvent`] to the [`Notification`] [`PruningPointUTXOSetOverrideNotification`]
    /// and potentially triggers a resync of the [`UtxoIndex`] (if the utxoindex is active).
    /// and sends the notification through the [`EventProcessor`] channel.
    async fn process_pruning_point_override_event(&self) -> EventProcessorResult<()> {
        trace!("[{IDENT}]: processing {:?}", PruningPointUTXOSetOverrideEvent {});
        if let Some(utxoindex) = self.utxoindex.as_deref() {
            utxoindex.write().resync()?;
        };

        self.rpc_send.send(Notification::PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideNotification {})).await?;
        Ok(())
    }

    /// Processes the [`ConsensusEvent`] [`FinalityConflictEvent`] to the [`Notification`] [`FinalityConflictNotification`]
    /// and sends this through the [`EventProcessor`] channel.
    async fn process_finality_conflict_event(&self) -> EventProcessorResult<()> {
        trace!("[{IDENT}]: processing {:?}", FinalityConflictEvent {});
        self.rpc_send.send(Notification::FinalityConflict(FinalityConflictNotification {})).await?;
        Ok(())
    }

    /// Processes the [`ConsensusEvent`] [`FinalityConflictResolvedEvent`] to the [`Notification`] [`FinalityConflictResolvedNotification`]
    /// and sends this through the [`EventProcessor`] channel.
    async fn process_finality_conflict_resolved_event(&self) -> EventProcessorResult<()> {
        trace!("[{IDENT}]: processing {:?}", FinalityConflictResolvedEvent {});
        self.rpc_send.send(Notification::FinalityConflictResolved(FinalityConflictResolvedNotification {})).await?;
        Ok(())
    }

    /// Potentially resyncs the [`UtxoIndex`] (if it is unsynced, and active) and starts the event processing loop.
    pub async fn run(&self) -> EventProcessorResult<()> {
        trace!("[{IDENT}]: initializing run...");
        if let Some(utxoindex) = self.utxoindex.as_deref() {
            if !utxoindex.read().is_synced()? {
                utxoindex.write().resync()?;
            }
        }
        let res = self.process_events().await;
        self.shutdown_finalized_trigger.trigger();
        res
    }
    /// Listens to Events, and matches these to corresponding processing methods,
    /// until a shutdown is signalled.
    async fn process_events(&self) -> EventProcessorResult<()> {
        trace!("[{IDENT}]: processing events...");
        let consensus_recv = self.consensus_recv.clone();
        let shutdown_listener = self.shutdown_listener.clone();

        loop {
            select! {

            _shutdown_signal = shutdown_listener.clone().fuse() => break,

            consensus_notification = consensus_recv.recv().fuse() => match consensus_notification? {
                        ConsensusEvent::NewBlockTemplate(_) => self.process_new_block_template_event().await?,
                        ConsensusEvent::VirtualChangeSet(virtual_change_set_event) => self.process_virtual_state_change_set_event(virtual_change_set_event).await?,
                        ConsensusEvent::BlockAdded(block_added_event) => self.process_block_added_event(block_added_event).await?,
                        ConsensusEvent::PruningPointUTXOSetOverride(_) => self.process_pruning_point_override_event().await?,
                        ConsensusEvent::FinalityConflict(_) => self.process_finality_conflict_event().await?,
                        ConsensusEvent::FinalityConflictResolved(_) => self.process_finality_conflict_resolved_event().await?,
                    },
                };
        }
        Ok(())
    }

    /// Triggers the shutdown, which breaks the async event processing loop, stopping all processing.
    pub fn signal_shutdown(&self) {
        self.shutdown_trigger.trigger();
    }
}
