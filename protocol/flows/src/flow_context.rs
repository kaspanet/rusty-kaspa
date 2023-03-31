use crate::flowcontext::{
    orphans::{OrphanBlocksPool, MAX_ORPHANS},
    process_queue::ProcessQueue,
    transactions::TransactionsSpread,
};
use crate::v5;
use async_trait::async_trait;
use kaspa_addressmanager::AddressManager;
use kaspa_consensus_core::{
    api::{ConsensusApi, DynConsensus},
    block::Block,
    config::Config,
    tx::{Transaction, TransactionId},
};
use kaspa_core::{debug, info, time::unix_now};
use kaspa_hashes::Hash;
use kaspa_mining::{
    manager::MiningManager,
    mempool::tx::{Orphan, Priority},
};
use kaspa_p2p_lib::{
    common::ProtocolError,
    pb::{self, KaspadMessage},
    ConnectionInitializer, Hub, KaspadHandshake, Router,
};
use parking_lot::Mutex;
use std::{
    collections::HashSet,
    iter::once,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::RwLock as AsyncRwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct FlowContext {
    pub consensus: DynConsensus,
    pub config: Config,
    hub: Hub,
    orphans_pool: Arc<AsyncRwLock<OrphanBlocksPool<dyn ConsensusApi>>>,
    shared_block_requests: Arc<Mutex<HashSet<Hash>>>,
    transactions_spread: Arc<AsyncRwLock<TransactionsSpread>>,
    shared_transaction_requests: Arc<Mutex<HashSet<TransactionId>>>,
    is_ibd_running: Arc<AtomicBool>, // TODO: pass the context wrapped with Arc and avoid some of the internal ones
    pub amgr: Arc<Mutex<AddressManager>>,
    mining_manager: Arc<MiningManager<dyn ConsensusApi>>,
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

impl FlowContext {
    pub fn new(
        consensus: DynConsensus,
        amgr: Arc<Mutex<AddressManager>>,
        config: &Config,
        mining_manager: Arc<MiningManager<dyn ConsensusApi>>,
    ) -> Self {
        let hub = Hub::new();
        Self {
            consensus: consensus.clone(),
            config: config.clone(),
            orphans_pool: Arc::new(AsyncRwLock::new(OrphanBlocksPool::new(consensus, MAX_ORPHANS))),
            shared_block_requests: Arc::new(Mutex::new(HashSet::new())),
            transactions_spread: Arc::new(AsyncRwLock::new(TransactionsSpread::new(hub.clone()))),
            shared_transaction_requests: Arc::new(Mutex::new(HashSet::new())),
            is_ibd_running: Arc::new(AtomicBool::default()),
            hub,
            amgr,
            mining_manager,
        }
    }

    pub fn consensus(&self) -> DynConsensus {
        self.consensus.clone()
    }

    pub fn hub(&self) -> Hub {
        self.hub.clone()
    }

    pub fn mining_manager(&self) -> Arc<MiningManager<dyn ConsensusApi>> {
        self.mining_manager.clone()
    }

    pub fn try_set_ibd_running(&self) -> Option<IbdRunningGuard> {
        if self.is_ibd_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            Some(IbdRunningGuard { indicator: self.is_ibd_running.clone() })
        } else {
            None
        }
    }

    pub fn is_ibd_running(&self) -> bool {
        self.is_ibd_running.load(Ordering::SeqCst)
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
        self.orphans_pool.write().await.add_orphan(orphan_block)
    }

    pub async fn is_known_orphan(&self, hash: Hash) -> bool {
        self.orphans_pool.read().await.is_known_orphan(hash)
    }

    pub async fn get_orphan_roots(&self, orphan: Hash) -> Option<Vec<Hash>> {
        self.orphans_pool.read().await.get_orphan_roots(orphan)
    }

    pub async fn unorphan_blocks(&self, root: Hash) -> Vec<Block> {
        self.orphans_pool.write().await.unorphan_blocks(root).await
    }

    /// Updates the mempool after a new block arrival, relays newly unorphaned transactions
    /// and possibly rebroadcast manually added transactions when not in IBD.
    ///
    /// _GO-KASPAD: OnNewBlock + broadcastTransactionsAfterBlockAdded_
    pub async fn on_new_block(&self, block: Block) -> Result<(), ProtocolError> {
        let hash = block.hash();
        let blocks = self.unorphan_blocks(hash).await;
        // Use a ProcessQueue so we get rid of duplicates
        let mut transactions_to_broadcast = ProcessQueue::new();
        for block in once(block).chain(blocks.into_iter()) {
            transactions_to_broadcast
                .enqueue_chunk(self.mining_manager().handle_new_block_transactions(&block.transactions)?.iter().map(|x| x.id()));
        }

        // Don't relay transactions when in IBD
        if self.is_ibd_running() {
            return Ok(());
        }

        if self.should_rebroadcast_transactions().await {
            transactions_to_broadcast.enqueue_chunk(self.mining_manager().revalidate_high_priority_transactions()?.into_iter());
        }

        self.broadcast_transactions(transactions_to_broadcast.drain(transactions_to_broadcast.len())).await
    }

    /// Notifies that a new block template is available for miners.
    pub async fn on_new_block_template(&self) -> Result<(), ProtocolError> {
        // Clear current template cache
        self.mining_manager().clear_block_template();
        // TODO: call a handler function or a predefined registered service
        Ok(())
    }

    /// Notifies that the UTXO set resets due to pruning point change via IBD.
    pub async fn on_pruning_point_utxoset_override(&self) -> Result<(), ProtocolError> {
        // TODO: call a handler function or a predefined registered service
        Ok(())
    }

    /// Notifies that a transaction has been added to the mempool.
    pub async fn on_transaction_added_to_mempool(&self) {
        // TODO: call a handler function or a predefined registered service
    }

    pub async fn add_transaction(&self, transaction: Transaction, orphan: Orphan) -> Result<(), ProtocolError> {
        let accepted_transactions = self.mining_manager().validate_and_insert_transaction(transaction, Priority::High, orphan)?;
        self.broadcast_transactions(accepted_transactions.iter().map(|x| x.id())).await
    }

    /// Returns true if the time for a rebroadcast of the mempool high priority transactions has come.
    ///
    /// If true, the instant of the call is registered as the last rebroadcast time.
    pub async fn should_rebroadcast_transactions(&self) -> bool {
        self.transactions_spread.write().await.should_rebroadcast_transactions()
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

    /// Broadcast a locally-originated message to all active network peers
    pub async fn broadcast(&self, msg: KaspadMessage) {
        self.hub.broadcast(msg).await;
    }
}

#[async_trait]
impl ConnectionInitializer for FlowContext {
    async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ProtocolError> {
        // Build the handshake object and subscribe to handshake messages
        let mut handshake = KaspadHandshake::new(&router);

        // We start the router receive loop only after we registered to handshake routes
        router.start();

        // Build the local version message
        // TODO: full and accurate version info
        let self_version_message = pb::VersionMessage {
            protocol_version: 5, // TODO: make a const
            services: 0,         // TODO: get number of live services
            timestamp: unix_now() as i64,
            address: None,                          // TODO
            id: Vec::from(Uuid::new_v4().as_ref()), // TODO
            user_agent: String::new(),              // TODO
            disable_relay_tx: false,                // TODO: config/cmd?
            subnetwork_id: None,                    // Subnets are not currently supported
            network: "kaspa-mainnet".to_string(),   // TODO: get network from config
        };

        // Perform the handshake
        let peer_version_message = handshake.handshake(self_version_message).await?;

        // TODO: verify the versions are compatible
        debug!("protocol versions - self: {}, peer: {}", 5, peer_version_message.protocol_version);

        // Register all flows according to version
        let flows = match peer_version_message.protocol_version {
            5 => v5::register(self.clone(), router.clone()),
            _ => todo!(),
        };

        // Send and receive the ready signal
        handshake.exchange_ready_messages().await?;

        info!("Registering p2p flows for peer {} for protocol version {}", router, peer_version_message.protocol_version);

        // Launch all flows. Note we launch only after the ready signal was exchanged
        for flow in flows {
            flow.launch();
        }

        if router.is_outbound() {
            self.amgr.lock().add_address(router.net_address().into());
        }

        // Note: we deliberately do not hold the handshake in memory so at this point receivers for handshake subscriptions
        // are dropped, hence effectively unsubscribing from these messages. This means that if the peer re-sends them
        // it is considered a protocol error and the connection will disconnect

        Ok(())
    }
}
