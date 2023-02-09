use crate::{
    errors::EventProcessorResult,
    notify::{
        BlockAddedNotification, FinalityConflictNotification, FinalityConflictResolvedNotification, NewBlockTemplateNotification,
        Notification, PruningPointUTXOSetOverrideNotification, UtxosChangedNotification, VirtualDaaScoreChangedNotification,
        VirtualSelectedParentBlueScoreChangedNotification, VirtualSelectedParentChainChangedNotification,
    },
};
use async_channel::{Receiver, Sender};
use consensus_core::events::{BlockAddedEvent, ConsensusEvent, VirtualChangeSetEvent};
use futures::{select, FutureExt};
use std::{sync::Arc, ops::Deref};
use triggered::{Listener, Trigger};
use utxoindex::{api::DynUtxoIndexControlerApi, UtxoIndex};
use utxoindex::events::UtxoIndexEvent;

#[derive(Clone)]
pub struct EventProcessor {
    utxoindex: DynUtxoIndexControlerApi,

    rpc_send: Sender<Notification>,
    consensus_recv: Receiver<ConsensusEvent>,

    shutdown_trigger: Trigger,
    shutdown_listener: Listener,

    shutdown_finalized_trigger: Trigger,
    pub shutdown_finalized_listener: Listener,
}

impl EventProcessor {
    pub fn new(utxoindex: DynUtxoIndexControlerApi, consensus_recv: Receiver<ConsensusEvent>, rpc_send: Sender<Notification>) -> Self {
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

    async fn process_new_block_template_event(&self) -> EventProcessorResult<()> {
        self.rpc_send.send(Notification::NewBlockTemplate(NewBlockTemplateNotification {})).await?;
        Ok(())
    }

    async fn process_block_added_event(&self, block_added_event: Arc<BlockAddedEvent>) -> EventProcessorResult<()> {
        self.rpc_send
            .send(Notification::BlockAdded(Arc::new(BlockAddedNotification { block: block_added_event.block.clone() })))
            .await?;
        Ok(())
    }

    async fn process_virtual_state_change_set_event(
        &self,
        virtual_change_set_event: Arc<VirtualChangeSetEvent>,
    ) -> EventProcessorResult<()> {
        match self.utxoindex.as_deref() {
            Some(utxoindex) => {
                match utxoindex.update(virtual_change_set_event.utxo_diff.clone(), virtual_change_set_event.parents.clone())? {
                    UtxoIndexEvent::UtxosChanged(utxo_changed_event) => {
                        self.rpc_send
                            .send(Notification::UtxosChanged(Arc::new(UtxosChangedNotification {
                                added: utxo_changed_event.added.clone(),
                                removed: utxo_changed_event.removed.clone(),
                            })))
                            .await?;
                    }
                }
            }
            None => (),
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

    async fn process_pruning_point_override_event(&self) -> EventProcessorResult<()> {
        match self.utxoindex.as_deref() {
            Some(utxoindex) => match utxoindex.resync() {
                Ok(_) => (),
                Err(err) => panic!("[Event-processor]: {err}"),
            },
            None => (),
        };

        self.rpc_send.send(Notification::PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideNotification {})).await?;
        Ok(())
    }

    async fn process_finality_conflict_event(&self) -> EventProcessorResult<()> {
        self.rpc_send.send(Notification::FinalityConflict(FinalityConflictNotification {})).await?;
        Ok(())
    }

    async fn process_finality_conflict_resolved_event(&self) -> EventProcessorResult<()> {
        self.rpc_send.send(Notification::FinalityConflictResolved(FinalityConflictResolvedNotification {})).await?;
        Ok(())
    }

    pub async fn run(&self) -> EventProcessorResult<()> {
        match self.utxoindex.as_deref() {
            Some(utxoindex) => {
                if !utxoindex.is_synced()? {
                    utxoindex.resync()?;
                }
            }
            None => (),
        };
        match self.process_events().await {
            Ok(_) => {
                self.shutdown_finalized_trigger.trigger();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }
    /// listens to consensus events, and preprocesses them for rpc-core, as well as controls the indexers.
    async fn process_events(&self) -> EventProcessorResult<()> {
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

    ///triggers the shutdown, which breaks the async event processing loop, stopping the processing.
    pub fn signal_shutdown(&self) {
        self.shutdown_trigger.trigger();
    }
}
