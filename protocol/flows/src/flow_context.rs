use crate::flowcontext::{
    orphans::{OrphanBlocksPool, OrphanOutput},
    process_queue::ProcessQueue,
    transactions::TransactionsSpread,
};
use crate::{v5, v6, v7, v8};
use async_trait::async_trait;
use futures::future::join_all;
use kaspa_addressmanager::AddressManager;
use kaspa_connectionmanager::ConnectionManager;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::tx::{Transaction, TransactionId};
use kaspa_consensus_core::{
    api::{BlockValidationFuture, BlockValidationFutures},
    network::NetworkType,
};
use kaspa_consensus_notify::{
    notification::{Notification, PruningPointUtxoSetOverrideNotification},
    root::ConsensusNotificationRoot,
};
use kaspa_consensusmanager::{BlockProcessingBatch, ConsensusInstance, ConsensusManager, ConsensusProxy, ConsensusSessionOwned};
use kaspa_core::{
    debug, info,
    kaspad_env::{name, version},
    task::tick::TickService,
};
use kaspa_core::{time::unix_now, warn};
use kaspa_hashes::Hash;
use kaspa_mining::mempool::tx::{Orphan, Priority};
use kaspa_mining::{manager::MiningManagerProxy, mempool::tx::RbfPolicy};
use kaspa_notify::notifier::Notify;
use kaspa_p2p_lib::{
    common::ProtocolError,
    convert::model::version::Version,
    make_message,
    pb::{kaspad_message::Payload, InvRelayBlockMessage},
    ConnectionInitializer, Hub, KaspadHandshake, PeerKey, PeerProperties, Router,
};
use kaspa_p2p_mining::rule_engine::MiningRuleEngine;
use kaspa_utils::iter::IterExtensions;
use kaspa_utils::networking::PeerId;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::time::Instant;
use std::{collections::hash_map::Entry, fmt::Display};
use std::{
    iter::once,
    ops::Deref,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    RwLock as AsyncRwLock,
};
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};
use uuid::Uuid;

/// The P2P protocol version.
const PROTOCOL_VERSION: u32 = 8;

/// See `check_orphan_resolution_range`
const BASELINE_ORPHAN_RESOLUTION_RANGE: u32 = 5;

/// Orphans are kept as full blocks so we cannot hold too much of them in memory
const MAX_ORPHANS_UPPER_BOUND: usize = 1024;

/// The min time to wait before allowing another parallel request
const REQUEST_SCOPE_WAIT_TIME: Duration = Duration::from_secs(1);

/// Represents a block event to be logged
#[derive(Debug, PartialEq)]
pub enum BlockLogEvent {
    /// Accepted block via *relay*
    Relay(Hash),
    /// Accepted block via *submit block*
    Submit(Hash),
    /// Orphaned block with x missing roots
    Orphaned(Hash, usize),
    /// Unorphaned x blocks with hash being a representative
    Unorphaned(Hash, usize),
}

pub struct BlockEventLogger {
    bps: usize,
    sender: UnboundedSender<BlockLogEvent>,
    receiver: Mutex<Option<UnboundedReceiver<BlockLogEvent>>>,
}

impl BlockEventLogger {
    pub fn new(bps: usize) -> Self {
        let (sender, receiver) = unbounded_channel();
        Self { bps, sender, receiver: Mutex::new(Some(receiver)) }
    }

    pub fn log(&self, event: BlockLogEvent) {
        self.sender.send(event).unwrap();
    }

    /// Start the logger listener. Must be called from an async tokio context
    fn start(&self) {
        let chunk_limit = self.bps * 10; // We prefer that the 1 sec timeout forces the log, but nonetheless still want a reasonable bound on each chunk
        let receiver = self.receiver.lock().take().expect("expected to be called once");
        tokio::spawn(async move {
            let chunk_stream = UnboundedReceiverStream::new(receiver).chunks_timeout(chunk_limit, Duration::from_secs(1));
            tokio::pin!(chunk_stream);
            while let Some(chunk) = chunk_stream.next().await {
                #[derive(Default)]
                struct LogSummary {
                    // Representatives
                    relay_rep: Option<Hash>,
                    submit_rep: Option<Hash>,
                    orphan_rep: Option<Hash>,
                    unorphan_rep: Option<Hash>,
                    // Counts
                    relay_count: usize,
                    submit_count: usize,
                    orphan_count: usize,
                    unorphan_count: usize,
                    orphan_roots_count: usize,
                }

                struct LogHash {
                    op: Option<Hash>,
                }

                impl From<Option<Hash>> for LogHash {
                    fn from(op: Option<Hash>) -> Self {
                        Self { op }
                    }
                }

                impl Display for LogHash {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        if let Some(hash) = self.op {
                            hash.fmt(f)
                        } else {
                            Ok(())
                        }
                    }
                }

                impl LogSummary {
                    fn relay(&self) -> LogHash {
                        self.relay_rep.into()
                    }

                    fn submit(&self) -> LogHash {
                        self.submit_rep.into()
                    }

                    fn orphan(&self) -> LogHash {
                        self.orphan_rep.into()
                    }

                    fn unorphan(&self) -> LogHash {
                        self.unorphan_rep.into()
                    }
                }

                let summary = chunk.into_iter().fold(LogSummary::default(), |mut summary, ev| {
                    match ev {
                        BlockLogEvent::Relay(hash) => {
                            summary.relay_count += 1;
                            summary.relay_rep = Some(hash)
                        }
                        BlockLogEvent::Submit(hash) => {
                            summary.submit_count += 1;
                            summary.submit_rep = Some(hash)
                        }
                        BlockLogEvent::Orphaned(hash, roots_count) => {
                            summary.orphan_roots_count += roots_count;
                            summary.orphan_count += 1;
                            summary.orphan_rep = Some(hash)
                        }
                        BlockLogEvent::Unorphaned(hash, count) => {
                            summary.unorphan_count += count;
                            summary.unorphan_rep = Some(hash)
                        }
                    }
                    summary
                });

                match (summary.submit_count, summary.relay_count) {
                    (0, 0) => {}
                    (1, 0) => info!("Accepted block {} via submit block", summary.submit()),
                    (n, 0) => info!("Accepted {} blocks ...{} via submit block", n, summary.submit()),
                    (0, 1) => info!("Accepted block {} via relay", summary.relay()),
                    (0, m) => info!("Accepted {} blocks ...{} via relay", m, summary.relay()),
                    (n, m) => {
                        info!("Accepted {} blocks ...{}, {} via relay and {} via submit block", n + m, summary.submit(), m, n)
                    }
                }

                match (summary.orphan_count, summary.orphan_roots_count) {
                    (0, 0) => {}
                    (n, m) => info!("Orphaned {} block(s) ...{} and queued {} missing roots", n, summary.orphan(), m),
                }

                match summary.unorphan_count {
                    0 => {}
                    1 => info!("Unorphaned block {}", summary.unorphan()),
                    n => info!("Unorphaned {} block(s) ...{}", n, summary.unorphan()),
                }
            }
        });
    }
}

pub struct FlowContextInner {
    pub node_id: PeerId,
    pub consensus_manager: Arc<ConsensusManager>,
    pub config: Arc<Config>,
    hub: Hub,
    orphans_pool: AsyncRwLock<OrphanBlocksPool>,
    shared_block_requests: Arc<Mutex<HashMap<Hash, RequestScopeMetadata>>>,
    transactions_spread: AsyncRwLock<TransactionsSpread>,
    shared_transaction_requests: Arc<Mutex<HashMap<TransactionId, RequestScopeMetadata>>>,
    is_ibd_running: Arc<AtomicBool>,
    ibd_metadata: Arc<RwLock<Option<IbdMetadata>>>,
    pub address_manager: Arc<Mutex<AddressManager>>,
    connection_manager: RwLock<Option<Arc<ConnectionManager>>>,
    mining_manager: MiningManagerProxy,
    pub(crate) tick_service: Arc<TickService>,
    notification_root: Arc<ConsensusNotificationRoot>,

    // Special sampling logger used only for high-bps networks where logs must be throttled
    block_event_logger: Option<BlockEventLogger>,

    // Bps upper bound
    bps_upper_bound: usize,

    // Orphan parameters
    orphan_resolution_range: u32,
    max_orphans: usize,

    // Mining rule engine
    mining_rule_engine: Arc<MiningRuleEngine>,
}

#[derive(Clone)]
pub struct FlowContext {
    inner: Arc<FlowContextInner>,
}

pub struct IbdRunningGuard {
    indicator: Arc<AtomicBool>,
}

impl Drop for IbdRunningGuard {
    fn drop(&mut self) {
        let result = self.indicator.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst);
        assert!(result.is_ok())
    }
}

#[derive(Debug, Clone, Copy)]
struct IbdMetadata {
    /// The peer from which current IBD is syncing from
    peer: PeerKey,
    /// The DAA score of the relay block which triggered the current IBD
    daa_score: u64,
}

pub struct RequestScopeMetadata {
    pub timestamp: Instant,
    pub obtained: bool,
}

pub struct RequestScope<T: PartialEq + Eq + std::hash::Hash> {
    set: Arc<Mutex<HashMap<T, RequestScopeMetadata>>>,
    pub req: T,
}

impl<T: PartialEq + Eq + std::hash::Hash> RequestScope<T> {
    pub fn new(set: Arc<Mutex<HashMap<T, RequestScopeMetadata>>>, req: T) -> Self {
        Self { set, req }
    }

    /// Scope holders should use this function to report that the request has
    /// successfully been obtained from the peer and is now being processed
    pub fn report_obtained(&self) {
        if let Some(e) = self.set.lock().get_mut(&self.req) {
            e.obtained = true;
        }
    }
}

impl<T: PartialEq + Eq + std::hash::Hash> Drop for RequestScope<T> {
    fn drop(&mut self) {
        self.set.lock().remove(&self.req);
    }
}

impl Deref for FlowContext {
    type Target = FlowContextInner;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

impl FlowContext {
    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        address_manager: Arc<Mutex<AddressManager>>,
        config: Arc<Config>,
        mining_manager: MiningManagerProxy,
        tick_service: Arc<TickService>,
        notification_root: Arc<ConsensusNotificationRoot>,
        hub: Hub,
        mining_rule_engine: Arc<MiningRuleEngine>,
    ) -> Self {
        let bps_upper_bound = config.bps().upper_bound() as usize;
        let orphan_resolution_range = BASELINE_ORPHAN_RESOLUTION_RANGE + (bps_upper_bound as f64).log2().ceil() as u32;

        // The maximum amount of orphans allowed in the orphans pool. This number is an approximation
        // of how many orphans there can possibly be on average bounded by an upper bound.
        let max_orphans =
            (2u64.pow(orphan_resolution_range) as usize * config.ghostdag_k().upper_bound() as usize).min(MAX_ORPHANS_UPPER_BOUND);
        Self {
            inner: Arc::new(FlowContextInner {
                node_id: Uuid::new_v4().into(),
                consensus_manager,
                orphans_pool: AsyncRwLock::new(OrphanBlocksPool::new(max_orphans)),
                shared_block_requests: Arc::new(Mutex::new(HashMap::new())),
                transactions_spread: AsyncRwLock::new(TransactionsSpread::new(hub.clone())),
                shared_transaction_requests: Arc::new(Mutex::new(HashMap::new())),
                is_ibd_running: Default::default(),
                ibd_metadata: Default::default(),
                hub,
                address_manager,
                connection_manager: Default::default(),
                mining_manager,
                tick_service,
                notification_root,
                block_event_logger: if bps_upper_bound > 1 { Some(BlockEventLogger::new(bps_upper_bound)) } else { None },
                bps_upper_bound,
                orphan_resolution_range,
                max_orphans,
                config,
                mining_rule_engine,
            }),
        }
    }

    pub fn block_invs_channel_size(&self) -> usize {
        self.bps_upper_bound * Router::incoming_flow_baseline_channel_size()
    }

    pub fn orphan_resolution_range(&self) -> u32 {
        self.orphan_resolution_range
    }

    pub fn max_orphans(&self) -> usize {
        self.max_orphans
    }

    pub fn start_async_services(&self) {
        if let Some(logger) = self.block_event_logger.as_ref() {
            logger.start();
        }
    }

    pub fn set_connection_manager(&self, connection_manager: Arc<ConnectionManager>) {
        self.connection_manager.write().replace(connection_manager);
    }

    pub fn drop_connection_manager(&self) {
        self.connection_manager.write().take();
    }

    pub fn connection_manager(&self) -> Option<Arc<ConnectionManager>> {
        self.connection_manager.read().clone()
    }

    pub fn consensus(&self) -> ConsensusInstance {
        self.consensus_manager.consensus()
    }

    pub fn hub(&self) -> &Hub {
        &self.hub
    }

    pub fn mining_manager(&self) -> &MiningManagerProxy {
        &self.mining_manager
    }

    pub fn try_set_ibd_running(&self, peer: PeerKey, relay_daa_score: u64) -> Option<IbdRunningGuard> {
        if self.is_ibd_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            self.ibd_metadata.write().replace(IbdMetadata { peer, daa_score: relay_daa_score });
            Some(IbdRunningGuard { indicator: self.is_ibd_running.clone() })
        } else {
            None
        }
    }

    pub fn is_ibd_running(&self) -> bool {
        self.is_ibd_running.load(Ordering::SeqCst)
    }

    /// If IBD is running, returns the IBD peer we are syncing from
    pub fn ibd_peer_key(&self) -> Option<PeerKey> {
        if self.is_ibd_running() {
            self.ibd_metadata.read().map(|md| md.peer)
        } else {
            None
        }
    }

    /// If IBD is running, returns the DAA score of the relay block which triggered it
    pub fn ibd_relay_daa_score(&self) -> Option<u64> {
        if self.is_ibd_running() {
            self.ibd_metadata.read().map(|md| md.daa_score)
        } else {
            None
        }
    }

    fn try_adding_request_impl(req: Hash, map: &Arc<Mutex<HashMap<Hash, RequestScopeMetadata>>>) -> Option<RequestScope<Hash>> {
        match map.lock().entry(req) {
            Entry::Occupied(mut e) => {
                if e.get().obtained {
                    None
                } else {
                    let now = Instant::now();
                    if now > e.get().timestamp + REQUEST_SCOPE_WAIT_TIME {
                        e.get_mut().timestamp = now;
                        Some(RequestScope::new(map.clone(), req))
                    } else {
                        None
                    }
                }
            }
            Entry::Vacant(e) => {
                e.insert(RequestScopeMetadata { timestamp: Instant::now(), obtained: false });
                Some(RequestScope::new(map.clone(), req))
            }
        }
    }

    pub fn try_adding_block_request(&self, req: Hash) -> Option<RequestScope<Hash>> {
        Self::try_adding_request_impl(req, &self.shared_block_requests)
    }

    pub fn try_adding_transaction_request(&self, req: TransactionId) -> Option<RequestScope<TransactionId>> {
        Self::try_adding_request_impl(req, &self.shared_transaction_requests)
    }

    pub async fn add_orphan(&self, consensus: &ConsensusProxy, orphan_block: Block) -> Option<OrphanOutput> {
        self.orphans_pool.write().await.add_orphan(consensus, orphan_block).await
    }

    pub async fn is_known_orphan(&self, hash: Hash) -> bool {
        self.orphans_pool.read().await.is_known_orphan(hash)
    }

    pub async fn get_orphan_roots_if_known(&self, consensus: &ConsensusProxy, orphan: Hash) -> OrphanOutput {
        self.orphans_pool.read().await.get_orphan_roots_if_known(consensus, orphan).await
    }

    pub async fn unorphan_blocks(&self, consensus: &ConsensusProxy, root: Hash) -> Vec<(Block, BlockValidationFuture)> {
        let (blocks, block_tasks, virtual_state_tasks) = self.orphans_pool.write().await.unorphan_blocks(consensus, root).await;
        let mut unorphaned_blocks = Vec::with_capacity(blocks.len());
        let results = join_all(block_tasks).await;
        for ((block, result), virtual_state_task) in blocks.into_iter().zip(results).zip(virtual_state_tasks) {
            match result {
                Ok(_) => {
                    unorphaned_blocks.push((block, virtual_state_task));
                }
                Err(e) => warn!("Validation failed for orphan block {}: {}", block.hash(), e),
            }
        }

        // Log or send to event logger
        if !unorphaned_blocks.is_empty() {
            if let Some(logger) = self.block_event_logger.as_ref() {
                logger.log(BlockLogEvent::Unorphaned(unorphaned_blocks[0].0.hash(), unorphaned_blocks.len()));
            } else {
                match unorphaned_blocks.len() {
                    1 => info!("Unorphaned block {}", unorphaned_blocks[0].0.hash()),
                    n => info!("Unorphaned {} blocks: {}", n, unorphaned_blocks.iter().map(|b| b.0.hash()).reusable_format(", ")),
                }
            }
        }
        unorphaned_blocks
    }

    pub async fn revalidate_orphans(&self, consensus: &ConsensusProxy) -> (Vec<Hash>, Vec<BlockValidationFuture>) {
        self.orphans_pool.write().await.revalidate_orphans(consensus).await
    }

    /// Adds the rpc-submitted block to the DAG and propagates it to peers.
    pub async fn submit_rpc_block(&self, consensus: &ConsensusProxy, block: Block) -> Result<(), ProtocolError> {
        if block.transactions.is_empty() {
            return Err(RuleError::NoTransactions)?;
        }
        let hash = block.hash();
        let BlockValidationFutures { block_task, virtual_state_task } = consensus.validate_and_insert_block(block.clone());
        if let Err(err) = block_task.await {
            warn!("Validation failed for block {}: {}", hash, err);
            return Err(err)?;
        }
        // Broadcast as soon as the block has been validated and inserted into the DAG
        self.hub.broadcast(make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(hash.into()) })).await;

        self.on_new_block(consensus, Default::default(), block, virtual_state_task).await;
        self.log_block_event(BlockLogEvent::Submit(hash));

        Ok(())
    }

    pub fn log_block_event(&self, event: BlockLogEvent) {
        if let Some(logger) = self.block_event_logger.as_ref() {
            logger.log(event)
        } else {
            match event {
                BlockLogEvent::Relay(hash) => info!("Accepted block {} via relay", hash),
                BlockLogEvent::Submit(hash) => info!("Accepted block {} via submit block", hash),
                BlockLogEvent::Orphaned(orphan, roots_count) => {
                    info!("Received a block with {} missing ancestors, adding to orphan pool: {}", roots_count, orphan)
                }
                _ => {}
            }
        }
    }

    /// Updates the mempool after a new block arrival, relays newly unorphaned transactions
    /// and possibly rebroadcast manually added transactions when not in IBD.
    ///
    /// _GO-KASPAD: OnNewBlock + broadcastTransactionsAfterBlockAdded_
    pub async fn on_new_block(
        &self,
        consensus: &ConsensusProxy,
        ancestor_batch: BlockProcessingBatch,
        block: Block,
        virtual_state_task: BlockValidationFuture,
    ) {
        let hash = block.hash();
        let mut blocks = self.unorphan_blocks(consensus, hash).await;

        // Broadcast unorphaned blocks
        let msgs = blocks
            .iter()
            .map(|(b, _)| make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(b.hash().into()) }))
            .collect();
        self.hub.broadcast_many(msgs).await;

        // Process blocks in topological order
        blocks.sort_by(|a, b| a.0.header.blue_work.partial_cmp(&b.0.header.blue_work).unwrap());
        // Use a ProcessQueue so we get rid of duplicates
        let mut transactions_to_broadcast = ProcessQueue::new();
        for (block, virtual_state_task) in ancestor_batch.zip().chain(once((block, virtual_state_task))).chain(blocks.into_iter()) {
            // We only care about waiting for virtual to process the block at this point, before proceeding with post-processing
            // actions such as updating the mempool. We know this will not err since `block_task` already completed w/o error
            let _ = virtual_state_task.await;
            if let Ok(txs) = self
                .mining_manager()
                .clone()
                .handle_new_block_transactions(consensus, block.header.daa_score, block.transactions.clone())
                .await
            {
                transactions_to_broadcast.enqueue_chunk(txs.into_iter().map(|x| x.id()));
            }
        }

        // Transaction relay is disabled if the node is out of sync
        if !self.is_nearly_synced(consensus).await {
            return;
        }

        // TODO: Throttle these transactions as well if needed
        self.broadcast_transactions(transactions_to_broadcast, false).await;

        if self.should_run_mempool_scanning_task().await {
            // Spawn a task executing the removal of expired low priority transactions and, if time has come too,
            // the revalidation of high priority transactions.
            //
            // The TransactionSpread member ensures at most one instance of this task is running at any
            // given time.
            let mining_manager = self.mining_manager().clone();
            let consensus_clone = consensus.clone();
            let context = self.clone();
            debug!("<> Starting mempool scanning task #{}...", self.mempool_scanning_job_count().await);
            tokio::spawn(async move {
                mining_manager.clone().expire_low_priority_transactions(&consensus_clone).await;
                if context.should_rebroadcast().await {
                    let (tx, mut rx) = unbounded_channel();
                    tokio::spawn(async move {
                        mining_manager.revalidate_high_priority_transactions(&consensus_clone, tx).await;
                    });
                    while let Some(transactions) = rx.recv().await {
                        let _ = context
                            .broadcast_transactions(
                                transactions,
                                true, // We throttle high priority even when the network is not flooded since they will be rebroadcast if not accepted within reasonable time.
                            )
                            .await;
                    }
                }
                context.mempool_scanning_is_done().await;
                debug!("<> Mempool scanning task is done");
            });
        }
    }

    pub async fn is_nearly_synced(&self, session: &ConsensusSessionOwned) -> bool {
        let sink_daa_score_and_timestamp = session.async_get_sink_daa_score_timestamp().await;
        self.mining_rule_engine.is_nearly_synced(sink_daa_score_and_timestamp)
    }

    pub async fn should_mine(&self, session: &ConsensusSessionOwned) -> bool {
        let sink_daa_score_and_timestamp = session.async_get_sink_daa_score_timestamp().await;
        self.mining_rule_engine.should_mine(sink_daa_score_and_timestamp)
    }

    /// Notifies that the UTXO set was reset due to pruning point change via IBD.
    pub fn on_pruning_point_utxoset_override(&self) {
        // Notifications from the flow context might be ignored if the inner channel is already closing
        // due to global shutdown, hence we ignore the possible error
        let _ = self.notification_root.notify(Notification::PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification {}));
    }

    /// Notifies that a transaction has been added to the mempool.
    pub async fn on_transaction_added_to_mempool(&self) {
        // TODO: call a handler function or a predefined registered service
    }

    /// Adds the rpc-submitted transaction to the mempool and propagates it to peers.
    ///
    /// Transactions submitted through rpc are considered high priority. This definition does not affect the tx selection algorithm
    /// but only changes how we manage the lifetime of the tx. A high-priority tx does not expire and is repeatedly rebroadcasted to
    /// peers
    pub async fn submit_rpc_transaction(
        &self,
        consensus: &ConsensusProxy,
        transaction: Transaction,
        orphan: Orphan,
    ) -> Result<(), ProtocolError> {
        let transaction_insertion = self
            .mining_manager()
            .clone()
            .validate_and_insert_transaction(consensus, transaction, Priority::High, orphan, RbfPolicy::Forbidden)
            .await?;
        self.broadcast_transactions(
            transaction_insertion.accepted.iter().map(|x| x.id()),
            false, // RPC transactions are considered high priority, so we don't want to throttle them
        )
        .await;
        Ok(())
    }

    /// Replaces the rpc-submitted transaction into the mempool and propagates it to peers.
    ///
    /// Returns the removed mempool transaction on successful replace by fee.
    ///
    /// Transactions submitted through rpc are considered high priority. This definition does not affect the tx selection algorithm
    /// but only changes how we manage the lifetime of the tx. A high-priority tx does not expire and is repeatedly rebroadcasted to
    /// peers
    pub async fn submit_rpc_transaction_replacement(
        &self,
        consensus: &ConsensusProxy,
        transaction: Transaction,
    ) -> Result<Arc<Transaction>, ProtocolError> {
        let transaction_insertion = self
            .mining_manager()
            .clone()
            .validate_and_insert_transaction(consensus, transaction, Priority::High, Orphan::Forbidden, RbfPolicy::Mandatory)
            .await?;
        self.broadcast_transactions(
            transaction_insertion.accepted.iter().map(|x| x.id()),
            false, // RPC transactions are considered high priority, so we don't want to throttle them
        )
        .await;
        // The combination of args above of Orphan::Forbidden and RbfPolicy::Mandatory should always result
        // in a removed transaction returned, however we prefer failing gracefully in case of future internal mempool changes
        transaction_insertion.removed.ok_or(ProtocolError::Other(
            "Replacement transaction was actually accepted but the *replaced* transaction was not returned from the mempool",
        ))
    }

    /// Returns true if the time has come for running the task cleaning mempool transactions.
    async fn should_run_mempool_scanning_task(&self) -> bool {
        self.transactions_spread.write().await.should_run_mempool_scanning_task()
    }

    /// Returns true if the time has come for a rebroadcast of the mempool high priority transactions.
    async fn should_rebroadcast(&self) -> bool {
        self.transactions_spread.read().await.should_rebroadcast()
    }

    async fn mempool_scanning_job_count(&self) -> u64 {
        self.transactions_spread.read().await.mempool_scanning_job_count()
    }

    async fn mempool_scanning_is_done(&self) {
        self.transactions_spread.write().await.mempool_scanning_is_done()
    }

    /// Add the given transactions IDs to a set of IDs to broadcast. The IDs will be broadcasted to all peers
    /// within transaction Inv messages.
    ///
    /// The broadcast itself may happen only during a subsequent call to this function since it is done at most
    /// after a predefined interval or when the queue length is larger than the Inv message capacity.
    pub async fn broadcast_transactions<I: IntoIterator<Item = TransactionId>>(&self, transaction_ids: I, should_throttle: bool) {
        self.transactions_spread.write().await.broadcast_transactions(transaction_ids, should_throttle).await
    }
}

#[async_trait]
impl ConnectionInitializer for FlowContext {
    async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ProtocolError> {
        // Build the handshake object and subscribe to handshake messages
        let mut handshake = KaspadHandshake::new(&router);

        // We start the router receive loop only after we registered to handshake routes
        router.start();

        let network_name = self.config.network_name();

        let local_address = self.address_manager.lock().best_local_address();

        // Build the local version message
        // Subnets are not currently supported
        let mut self_version_message = Version::new(local_address, self.node_id, network_name.clone(), None, PROTOCOL_VERSION);
        self_version_message.add_user_agent(name(), version(), &self.config.user_agent_comments);
        // TODO: get number of live services
        // TODO: disable_relay_tx from config/cmd

        // Perform the handshake
        let peer_version_message = handshake.handshake(self_version_message.into()).await?;
        // Get time_offset as accurate as possible by computing right after the handshake
        let time_offset = unix_now() as i64 - peer_version_message.timestamp;

        let peer_version: Version = peer_version_message.try_into()?;
        router.set_identity(peer_version.id);
        // Avoid duplicate connections
        if self.hub.has_peer(router.key()) {
            return Err(ProtocolError::PeerAlreadyExists(router.key()));
        }
        // And loopback connections...
        if self.node_id == router.identity() {
            return Err(ProtocolError::LoopbackConnection(router.key()));
        }

        if peer_version.network != network_name {
            return Err(ProtocolError::WrongNetwork(network_name, peer_version.network));
        }

        debug!("protocol versions - self: {}, peer: {}", PROTOCOL_VERSION, peer_version.protocol_version);

        // Register all flows according to version
        let connect_only_new_versions = self.config.net.network_type() != NetworkType::Testnet;

        let (flows, applied_protocol_version) = if connect_only_new_versions {
            match peer_version.protocol_version {
                v if v >= PROTOCOL_VERSION => (v8::register(self.clone(), router.clone()), PROTOCOL_VERSION),
                7 => (v7::register(self.clone(), router.clone()), 7),
                v => return Err(ProtocolError::VersionMismatch(PROTOCOL_VERSION, v)),
            }
        } else {
            match peer_version.protocol_version {
                v if v >= PROTOCOL_VERSION => (v8::register(self.clone(), router.clone()), PROTOCOL_VERSION),
                7 => (v7::register(self.clone(), router.clone()), 7),
                6 => (v6::register(self.clone(), router.clone()), 6),
                5 => (v5::register(self.clone(), router.clone()), 5),
                v => return Err(ProtocolError::VersionMismatch(PROTOCOL_VERSION, v)),
            }
        };

        // Build and register the peer properties
        let peer_properties = Arc::new(PeerProperties {
            user_agent: peer_version.user_agent.to_owned(),
            advertised_protocol_version: peer_version.protocol_version,
            protocol_version: applied_protocol_version,
            disable_relay_tx: peer_version.disable_relay_tx,
            subnetwork_id: peer_version.subnetwork_id.to_owned(),
            time_offset,
        });
        router.set_properties(peer_properties);

        // Send and receive the ready signal
        handshake.exchange_ready_messages().await?;

        info!("Registering p2p flows for peer {} for protocol version {}", router, applied_protocol_version);

        // Launch all flows. Note we launch only after the ready signal was exchanged
        for flow in flows {
            flow.launch();
        }

        if router.is_outbound() || peer_version.address.is_some() {
            let mut address_manager = self.address_manager.lock();

            if router.is_outbound() {
                address_manager.add_address(router.net_address().into());
            }

            if let Some(peer_ip_address) = peer_version.address {
                address_manager.add_address(peer_ip_address);
            }
        }

        // Note: we deliberately do not hold the handshake in memory so at this point receivers for handshake subscriptions
        // are dropped, hence effectively unsubscribing from these messages. This means that if the peer re-sends them
        // it is considered a protocol error and the connection will disconnect

        Ok(())
    }
}
