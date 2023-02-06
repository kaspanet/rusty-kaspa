use super::rpc_collector::*;
use addresses::Prefix;
use async_trait::async_trait;
use consensus_core::{
    networktype::NetworkType,
    notify::{
        BlockAddedNotification, ConsensusNotification, FinalityConflictResolvedNotification, NewBlockTemplateNotification,
        PruningPointUTXOSetOverrideNotification, VirtualChangeSetNotification,
    },
};
use futures::{
    future::FutureExt, // for `.fuse()`
    pin_mut,
    select,
};
use futures_util::stream::StreamExt;
use kaspa_core::trace;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use utxoindex::api::DynUtxoIndex;
extern crate derive_more;
use crate::{
    notify::{collector::rpc_collector, error::Error, notifier::Notifier, result::Result},
    Notification,
};
use crate::{
    stubs::{
        FinalityConflictNotification, VirtualDaaScoreChangedNotification, VirtualSelectedParentBlueScoreChangedNotification,
        VirtualSelectedParentChainChangedNotification,
    },
    RpcResult,
};
use derivative::Derivative;
use kaspa_utils::triggers::DuplexTrigger;

/// A specilized consensus notification [`Collector`] that receives [`ConsensusNotification`] from a channel,
/// processes and converts it into a [`Notification`] and sends it to a its
/// [`Notifier`].
/// 
/// note: this collector has been redesigned to work akin to go-kaspad's rpc-manager via a notify methods in a seperate [`ConsensusCollectorNotify`] trait.
/// this allows for more extensive pre-processing, and control over indexers, a separate abstracted trait [`RpcNotfiyApi`] has also been implemented over 
/// over [`RpcCoreService`] which allows for call-back bases insertion of notifications.  

#[derive(Derivative)]
#[derivative(Debug)]
pub struct ConsensusCollector {
    recv_channel: CollectorNotificationReceiver<ConsensusNotification>,

    #[derivative(Debug = "ignore")]
    utxoindex: DynUtxoIndex, // TODO: move into a context eventually

    /// Has this collector been started?
    is_started: Arc<AtomicBool>,

    collect_shutdown: Arc<DuplexTrigger>,
}

impl ConsensusCollector {
    pub fn new(recv_channel: CollectorNotificationReceiver<ConsensusNotification>, utxoindex: DynUtxoIndex) -> Self {
        Self {
            recv_channel,
            collect_shutdown: Arc::new(DuplexTrigger::new()),
            is_started: Arc::new(AtomicBool::new(false)),
            utxoindex,
        }
    }

    fn spawn_collecting_task(self: Arc<Self>, notifier: Arc<Notifier>) {
        // The task can only be spawned once
        if self.is_started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        let collect_shutdown = self.collect_shutdown.clone();
        let recv_channel = self.recv_channel.clone();
        let utxoindex = self.utxoindex.clone();

        workflow_core::task::spawn(async move {
            trace!("[Collector] collecting_task start");

            let shutdown = collect_shutdown.request.listener.clone().fuse();
            pin_mut!(shutdown);

            let notifications = recv_channel.fuse();
            pin_mut!(notifications);

            loop {
                select! {
                    _ = shutdown => { break; }
                    notification = notifications.next().fuse() => match notification {
                        Some(msg) => {
                            match msg {
                                ConsensusNotification::NewBlockTemplate(new_block_template_notification) => {
                                    match self.notify_new_block_template(new_block_template_notification, notifier) {
                                        Ok(_) => (),
                                        Err(err) => {
                                            trace!("[ConsensusCollector] notification sender error: {:?}, for {:?}", err, new_block_template_notification);
                                        },
                                    }
                                }
                                ConsensusNotification::BlockAdded(block_added_notification) => {
                                    match self.notify_block_added_to_dag(block_added_notification, notifier) {
                                        Ok(_) => (),
                                        Err(err) => {
                                            trace!("[ConsensusCollector] notification sender error: {:?}, for {:?}", err, block_added_notification);
                                        },
                                    }
                                }
                                ConsensusNotification::VirtualChangeSet(virtual_change_set_notification) => {
                                    match self.notify_virtual_change(virtual_change_set_notification, notifier, Prefix::Mainnet) {
                                        Ok(_) => (),
                                        Err(err) => {
                                            trace!("[ConsensusCollector] notification sender error: {:?}, for {:?}", err, virtual_change_set_notification);
                                        },
                                    }
                                }
                                ConsensusNotification::PruningPointUTXOSetOverride(prunting_point_utxo_set_override_notification) => {
                                    match self.notify_pruning_point_utxo_set_override(prunting_point_utxo_set_override_notification, notifier) {
                                        Ok(_) => (),
                                        Err(err) => {
                                            trace!("[ConsensusCollector] notification sender error: {:?}, for {:?}", err, prunting_point_utxo_set_override_notification);
                                        },
                                    }
                                }
                                ConsensusNotification::FinalityConflictResolved(finality_conflict_resolved_notification) => {
                                    match self.notify_finality_conflict_resolved(finality_conflict_resolved_notification, notifier) {
                                        Ok(_) => (),
                                        Err(err) => {
                                            trace!("[ConsensusCollector] notification sender error: {:?}, for {:?}", err, finality_conflict_resolved_notification);
                                        },
                                    }
                                }
                                ConsensusNotification::FinalityConflicts(finanlity_conflicts_notfication) => {
                                    match self.notify_finality_conflict(finanlity_conflicts_notfication, notifier) {
                                        Ok(_) => (),
                                        Err(err) => {
                                            trace!("[ConsensusCollector] notification sender error: {:?}, for {:?}", err, finanlity_conflicts_notfication);
                                        },
                                    }
                                }
                            }
                        }
                        None => {
                            trace!("[ConsensusCollector] notifications returned None. This should never happen");
                        }
                    },
                }
            }
        });
        collect_shutdown.response.trigger.trigger();
        trace!("[Collector] collecting_task end");
    }

    async fn stop_collecting_task(self: Arc<Self>) -> RpcResult<()> {
        if self.is_started.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Err(Error::AlreadyStoppedError);
        }
        self.collect_shutdown.request.trigger.trigger();
        self.collect_shutdown.response.listener.clone().await;
        Ok(())
    }
}

#[async_trait]
impl rpc_collector::Collector for ConsensusCollector {
    fn start(self: Arc<Self>, notifier: Arc<Notifier>) {
        self.spawn_collecting_task(notifier);
    }

    async fn stop(self: Arc<Self>) -> RpcResult<()> {
        self.stop_collecting_task().await
    }
}
/// Represents a set of [`ConsensusCollector`] methods, for greater pre-processing control, these can also be called without a channel via
/// the abstracted [`RpcNotfiyApi`] trait from external components.  
pub trait ConsensusCollectorNotify {
    fn notify_block_added_to_dag(&self, block_added: BlockAddedNotification, notifier: Arc<Notifier>) -> RpcResult<()>;

    fn notify_virtual_change(
        &self,
        virtual_change_set: VirtualChangeSetNotification,
        notifier: Arc<Notifier>,
        prefix: Prefix,
    ) -> RpcResult<()>;

    fn notify_new_block_template(&self, new_block_template: NewBlockTemplateNotification, notifier: Arc<Notifier>) -> RpcResult<()>;

    fn notify_finality_conflict(&self, finality_conflict: FinalityConflictNotification, notifier: Arc<Notifier>) -> RpcResult<()>;

    fn notify_finality_conflict_resolved(
        &self,
        finality_conflict_resolved: FinalityConflictResolvedNotification,
        notfier: Arc<Notifier>,
    ) -> RpcResult<()>;

    fn notify_pruning_point_utxo_set_override(
        &self,
        pruning_point_override: PruningPointUTXOSetOverrideNotification,
        notifier: Arc<Notifier>,
    ) -> RpcResult<()>;
}

impl ConsensusCollectorNotify for ConsensusCollector {
    fn notify_block_added_to_dag(&self, block_added: Arc<BlockAddedNotification>, notifier: Arc<Notifier>) -> RpcResult<()> {
        notifier.notify(ArcConvert::from(block_added).into())
    }

    fn notify_virtual_change(
        &self,
        virtual_change_set: VirtualChangeSetNotification,
        notifier: Arc<Notifier>,
        prefix: Prefix,
    ) -> RpcResult<()> {
        //TODO: skip if utxoindex is disabled, this requires context.
        for utxoindex_notification in
            (self.utxoindex.update(virtual_change_set.virtual_utxo_diff, virtual_change_set.virtual_parents)?).into_iter()
        {
            match utxoindex_notification {
                utxoindex::notify::UtxoIndexNotification::UtxoChanges(utxo_changes) => {
                    // Note (Maybe TODO): the subtile `.into()` below creates a bigger change compared to go-kaspad.
                    // essentially we are converting from a UtxoChanges to UtxoChanged notification outside of the listeners.
                    // the UtxoChanged does not utilize a hashmap, which negates an optimization for listeners listening to a small amount of utxos.
                    // UtxoChanged also has some more bloated data i.e. the address.
                    // despite this, it does have added benifit of not potentially needing to run txscript address conversion(s) multiple times in individual listeners
                    //
                    // Utilizing UtxoChanges in the listener's utxo address filter makes conventions blurry regarding Notifications and events,
                    // 
                    // it will also require some overhead in redesigning the core notifier to have a special fall-through case for UtxoChanges Notfictations,
                    // then it can be converted inside the protocol specific notfier's listeners to UtxoChanged Notfications. 
                    // as such, this potential optimization is left out for now.
                    notifier.notify(Arc::new(Notification::UtxosChanged((utxo_changes, prefix).try_into())));
                }
            }
        }

        notifier.notify(Arc::new(Notification::VirtualDaaScoreChanged(virtual_change_set.virtual_daa_score)))?;
        notifier.notify(Arc::new(Notification::VirtualSelectedParentBlueScoreChanged(
            virtual_change_set.virtual_selected_parent_blue_score,
        )))?;
        notifier.notify(Arc::new(Notification::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedNotification {})))
    }

    fn notify_new_block_template(&self, new_block_template: NewBlockTemplateNotification, notifier: Arc<Notifier>) -> Result<()> {
        notifier.notify(Arc::new(Notification::NewBlockTemplate(new_block_template)))
    }

    fn notify_finality_conflict(&self, finality_conflict: FinalityConflictNotification, notifier: Arc<Notifier>) -> Result<()> {
        notifier.notify(Arc::new(Notification::FinalityConflict(finality_conflict)))
    }

    fn notify_finality_conflict_resolved(
        &self,
        finality_conflict_resolved: FinalityConflictResolvedNotification,
        notifier: Arc<Notifier>,
    ) -> Result<()> {
        notifier.notify(Arc::new(Notification::FinalityConflictResolved(finality_conflict_resolved)))
    }

    fn notify_pruning_point_utxo_set_override(
        &self,
        pruning_point_override: PruningPointUTXOSetOverrideNotification,
        notifier: Arc<Notifier>,
    ) -> Result<()> {
        self.utxoindex.reset()?;
        notifier.notify(Arc::new(Notification::PruningPointUTXOSetOverride(pruning_point_override)))
    }
}

type DynConsensusCollectorNotfier = Arc<dyn ConsensusCollectorNotify>;
