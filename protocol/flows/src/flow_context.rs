use crate::flowcontext::{orphans::OrphanBlocksPool, process_queue::ProcessQueue, transactions::TransactionsSpread};
use crate::v5;
use async_trait::async_trait;
use kaspa_addressmanager::AddressManager;
use kaspa_connectionmanager::ConnectionManager;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::tx::{Transaction, TransactionId};
use kaspa_consensus_notify::{
    notification::{NewBlockTemplateNotification, Notification, PruningPointUtxoSetOverrideNotification},
    root::ConsensusNotificationRoot,
};
use kaspa_consensusmanager::{ConsensusInstance, ConsensusManager, ConsensusProxy};
use kaspa_core::{
    debug, info,
    kaspad_env::{name, version},
    task::tick::TickService,
};
use kaspa_core::{time::unix_now, warn};
use kaspa_hashes::Hash;
use kaspa_mining::manager::MiningManagerProxy;
use kaspa_mining::mempool::tx::{Orphan, Priority};
use kaspa_notify::notifier::Notify;
use kaspa_p2p_lib::{
    common::ProtocolError,
    convert::model::version::Version,
    make_message,
    pb::{kaspad_message::Payload, InvRelayBlockMessage},
    ConnectionInitializer, Hub, KaspadHandshake, PeerKey, PeerProperties, Router,
};
use kaspa_utils::iter::IterExtensions;
use kaspa_utils::networking::PeerId;
use parking_lot::{Mutex, RwLock};
use std::{
    collections::HashSet,
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

/// The P2P protocol version. Currently the only one supported.
const PROTOCOL_VERSION: u32 = 5;

/// See `check_orphan_resolution_range`
const BASELINE_ORPHAN_RESOLUTION_RANGE: u32 = 5;

#[derive(Debug, PartialEq)]
pub enum BlockSource {
    Relay,
    Submit,
}

pub struct AcceptedBlockLogger {
    bps: usize,
    sender: UnboundedSender<(Hash, BlockSource)>,
    receiver: Mutex<Option<UnboundedReceiver<(Hash, BlockSource)>>>,
}

impl AcceptedBlockLogger {
    pub fn new(bps: usize) -> Self {
        let (sender, receiver) = unbounded_channel();
        Self { bps, sender, receiver: Mutex::new(Some(receiver)) }
    }

    pub fn log(&self, hash: Hash, source: BlockSource) {
        self.sender.send((hash, source)).unwrap();
    }

    /// Start the logger listener. Must be called from an async tokio context
    fn start(&self) {
        let chunk_limit = self.bps * 4; // We prefer that the 1 sec timeout forces the log, but nonetheless still want a reasonable bound on each chunk
        let receiver = self.receiver.lock().take().expect("expected to be called once");
        tokio::spawn(async move {
            let chunk_stream = UnboundedReceiverStream::new(receiver).chunks_timeout(chunk_limit, Duration::from_secs(1));
            tokio::pin!(chunk_stream);
            while let Some(chunk) = chunk_stream.next().await {
                if let Some((i, h)) =
                    chunk.iter().filter_map(|(h, s)| if *s == BlockSource::Submit { Some(*h) } else { None }).enumerate().last()
                {
                    let submit = i + 1; // i is the last index so i + 1 is the number of submit blocks
                    let relay = chunk.len() - submit;
                    match (submit, relay) {
                        (1, 0) => info!("Accepted block {} via submit block", h),
                        (n, 0) => info!("Accepted {} blocks ...{} via submit block", n, h),
                        (n, m) => info!("Accepted {} blocks ...{}, {} via relay and {} via submit block", n + m, h, m, n),
                    }
                } else {
                    let h = chunk.last().expect("chunk is never empty").0;
                    match chunk.len() {
                        1 => info!("Accepted block {} via relay", h),
                        n => info!("Accepted {} blocks ...{} via relay", n, h),
                    }
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
    shared_block_requests: Arc<Mutex<HashSet<Hash>>>,
    transactions_spread: AsyncRwLock<TransactionsSpread>,
    shared_transaction_requests: Arc<Mutex<HashSet<TransactionId>>>,
    is_ibd_running: Arc<AtomicBool>,
    ibd_peer_key: Arc<RwLock<Option<PeerKey>>>,
    pub address_manager: Arc<Mutex<AddressManager>>,
    connection_manager: RwLock<Option<Arc<ConnectionManager>>>,
    mining_manager: MiningManagerProxy,
    pub(crate) tick_service: Arc<TickService>,
    pub(crate) notification_root: Arc<ConsensusNotificationRoot>,

    // Special sampling logger used only for high-bps networks where logs must be throttled
    accepted_block_logger: Option<AcceptedBlockLogger>,

    // Orphan parameters
    orphan_resolution_range: u32,
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

pub struct RequestScope<T: PartialEq + Eq + std::hash::Hash> {
    set: Arc<Mutex<HashSet<T>>>,
    pub req: T,
}

impl<T: PartialEq + Eq + std::hash::Hash> RequestScope<T> {
    pub fn new(set: Arc<Mutex<HashSet<T>>>, req: T) -> Self {
        Self { set, req }
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
    ) -> Self {
        let hub = Hub::new();

        let orphan_resolution_range = BASELINE_ORPHAN_RESOLUTION_RANGE + (config.bps() as f64).log2().min(3.0) as u32;

        // The maximum amount of orphans allowed in the orphans pool. This number is an
        // approximation of how many orphans there can possibly be on average.
        let max_orphans = 2u64.pow(orphan_resolution_range) as usize * config.ghostdag_k as usize;
        Self {
            inner: Arc::new(FlowContextInner {
                node_id: Uuid::new_v4().into(),
                consensus_manager,
                orphans_pool: AsyncRwLock::new(OrphanBlocksPool::new(max_orphans)),
                shared_block_requests: Arc::new(Mutex::new(HashSet::new())),
                transactions_spread: AsyncRwLock::new(TransactionsSpread::new(hub.clone())),
                shared_transaction_requests: Arc::new(Mutex::new(HashSet::new())),
                is_ibd_running: Default::default(),
                ibd_peer_key: Default::default(),
                hub,
                address_manager,
                connection_manager: Default::default(),
                mining_manager,
                tick_service,
                notification_root,
                accepted_block_logger: if config.bps() > 1 { Some(AcceptedBlockLogger::new(config.bps() as usize)) } else { None },
                orphan_resolution_range,
                config,
            }),
        }
    }

    pub fn block_invs_channel_size(&self) -> usize {
        self.config.bps() as usize * Router::incoming_flow_baseline_channel_size()
    }

    pub fn orphan_resolution_range(&self) -> u32 {
        self.orphan_resolution_range
    }

    pub fn start_async_services(&self) {
        if let Some(logger) = self.accepted_block_logger.as_ref() {
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

    pub fn try_set_ibd_running(&self, peer_key: PeerKey) -> Option<IbdRunningGuard> {
        if self.is_ibd_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            self.ibd_peer_key.write().replace(peer_key);
            Some(IbdRunningGuard { indicator: self.is_ibd_running.clone() })
        } else {
            None
        }
    }

    pub fn is_ibd_running(&self) -> bool {
        self.is_ibd_running.load(Ordering::SeqCst)
    }

    pub fn ibd_peer_key(&self) -> Option<PeerKey> {
        if self.is_ibd_running() {
            *self.ibd_peer_key.read()
        } else {
            None
        }
    }

    pub fn try_adding_block_request(&self, req: Hash) -> Option<RequestScope<Hash>> {
        if self.shared_block_requests.lock().insert(req) {
            Some(RequestScope::new(self.shared_block_requests.clone(), req))
        } else {
            None
        }
    }

    pub fn try_adding_transaction_request(&self, req: TransactionId) -> Option<RequestScope<TransactionId>> {
        if self.shared_transaction_requests.lock().insert(req) {
            Some(RequestScope::new(self.shared_transaction_requests.clone(), req))
        } else {
            None
        }
    }

    pub async fn add_orphan(&self, orphan_block: Block) {
        if self.is_log_throttled() {
            debug!("Received a block with missing parents, adding to orphan pool: {}", orphan_block.hash());
        } else {
            info!("Received a block with missing parents, adding to orphan pool: {}", orphan_block.hash());
        }
        self.orphans_pool.write().await.add_orphan(orphan_block)
    }

    pub async fn is_known_orphan(&self, hash: Hash) -> bool {
        self.orphans_pool.read().await.is_known_orphan(hash)
    }

    pub async fn get_orphan_roots(&self, consensus: &ConsensusProxy, orphan: Hash) -> Option<Vec<Hash>> {
        self.orphans_pool.read().await.get_orphan_roots(consensus, orphan).await
    }

    pub async fn unorphan_blocks(&self, consensus: &ConsensusProxy, root: Hash) -> Vec<Block> {
        let unorphaned_blocks = self.orphans_pool.write().await.unorphan_blocks(consensus, root).await;
        match unorphaned_blocks.len() {
            0 => {}
            1 => info!("Unorphaned block {}", unorphaned_blocks[0].hash()),
            n => match self.is_log_throttled() {
                true => info!("Unorphaned {} blocks ...{}", n, unorphaned_blocks.last().unwrap().hash()),
                false => info!("Unorphaned {} blocks: {}", n, unorphaned_blocks.iter().map(|b| b.hash()).reusable_format(", ")),
            },
        }
        unorphaned_blocks
    }

    /// Adds the rpc-submitted block to the DAG and propagates it to peers.
    pub async fn submit_rpc_block(&self, consensus: &ConsensusProxy, block: Block) -> Result<(), ProtocolError> {
        if block.transactions.is_empty() {
            return Err(RuleError::NoTransactions)?;
        }
        let hash = block.hash();
        if let Err(err) = self.consensus().session().await.validate_and_insert_block(block.clone()).await {
            warn!("Validation failed for block {}: {}", hash, err);
            return Err(err)?;
        }
        self.log_block_acceptance(hash, BlockSource::Submit);
        self.on_new_block_template().await?;
        self.on_new_block(consensus, block).await?;
        self.hub.broadcast(make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(hash.into()) })).await;
        Ok(())
    }

    pub fn log_block_acceptance(&self, hash: Hash, source: BlockSource) {
        if let Some(logger) = self.accepted_block_logger.as_ref() {
            logger.log(hash, source)
        } else {
            match source {
                BlockSource::Relay => info!("Accepted block {} via relay", hash),
                BlockSource::Submit => info!("Accepted block {} via submit block", hash),
            }
        }
    }

    pub fn is_log_throttled(&self) -> bool {
        self.accepted_block_logger.is_some()
    }

    /// Updates the mempool after a new block arrival, relays newly unorphaned transactions
    /// and possibly rebroadcast manually added transactions when not in IBD.
    ///
    /// _GO-KASPAD: OnNewBlock + broadcastTransactionsAfterBlockAdded_
    pub async fn on_new_block(&self, consensus: &ConsensusProxy, block: Block) -> Result<(), ProtocolError> {
        let hash = block.hash();
        let mut blocks = self.unorphan_blocks(consensus, hash).await;
        // Process blocks in topological order
        blocks.sort_by(|a, b| a.header.blue_work.partial_cmp(&b.header.blue_work).unwrap());
        // Use a ProcessQueue so we get rid of duplicates
        let mut transactions_to_broadcast = ProcessQueue::new();
        for block in once(block).chain(blocks.into_iter()) {
            transactions_to_broadcast.enqueue_chunk(
                self.mining_manager()
                    .clone()
                    .handle_new_block_transactions(consensus, block.header.daa_score, block.transactions.clone())
                    .await?
                    .iter()
                    .map(|x| x.id()),
            );
        }

        // Don't relay transactions when in IBD
        if self.is_ibd_running() {
            return Ok(());
        }

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
                        let _ = context.broadcast_transactions(transactions).await;
                    }
                }
                context.mempool_scanning_is_done().await;
                debug!("<> Mempool scanning task is done");
            });
        }

        self.broadcast_transactions(transactions_to_broadcast).await
    }

    /// Notifies that a new block template is available for miners.
    pub async fn on_new_block_template(&self) -> Result<(), ProtocolError> {
        // Clear current template cache
        self.mining_manager().clear_block_template();
        // Notifications from the flow context might be ignored if the inner channel is already closing
        // due to global shutdown, hence we ignore the possible error
        let _ = self.notification_root.notify(Notification::NewBlockTemplate(NewBlockTemplateNotification {}));
        Ok(())
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
        let accepted_transactions =
            self.mining_manager().clone().validate_and_insert_transaction(consensus, transaction, Priority::High, orphan).await?;
        self.broadcast_transactions(accepted_transactions.iter().map(|x| x.id())).await
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
    pub async fn broadcast_transactions<I: IntoIterator<Item = TransactionId>>(
        &self,
        transaction_ids: I,
    ) -> Result<(), ProtocolError> {
        self.transactions_spread.write().await.broadcast_transactions(transaction_ids).await
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
        let (flows, applied_protocol_version) = match peer_version.protocol_version {
            PROTOCOL_VERSION => (v5::register(self.clone(), router.clone()), PROTOCOL_VERSION),
            // TODO: different errors for obsolete (low version) vs unknown (high)
            v => return Err(ProtocolError::VersionMismatch(PROTOCOL_VERSION, v)),
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

        info!("Registering p2p flows for peer {} for protocol version {}", router, peer_version.protocol_version);

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
