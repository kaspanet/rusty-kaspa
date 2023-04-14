use crate::flowcontext::{
    orphans::{OrphanBlocksPool, MAX_ORPHANS},
    process_queue::ProcessQueue,
    transactions::TransactionsSpread,
};
use crate::v5;
use async_trait::async_trait;
use kaspa_addressmanager::AddressManager;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::tx::{Transaction, TransactionId};
use kaspa_consensus_core::{api::ConsensusApi, errors::block::RuleError};
use kaspa_consensus_notify::{
    notification::{NewBlockTemplateNotification, Notification},
    root::ConsensusNotificationRoot,
};
use kaspa_consensusmanager::{ConsensusInstance, ConsensusManager};
use kaspa_core::{debug, info};
use kaspa_core::{time::unix_now, warn};
use kaspa_hashes::Hash;
use kaspa_mining::{
    manager::MiningManager,
    mempool::tx::{Orphan, Priority},
};
use kaspa_notify::notifier::Notify;
use kaspa_p2p_lib::{
    common::ProtocolError,
    make_message,
    pb::{self, kaspad_message::Payload, InvRelayBlockMessage},
    ConnectionInitializer, Hub, KaspadHandshake, Router,
};
use parking_lot::Mutex;
use std::{
    collections::HashSet,
    iter::once,
    ops::Deref,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::RwLock as AsyncRwLock;
use uuid::Uuid;

/// The P2P protocol version. Currently the only one supported.
const PROTOCOL_VERSION: u32 = 5;

pub struct FlowContextInner {
    pub node_id: Uuid,
    pub consensus_manager: Arc<ConsensusManager>,
    pub config: Arc<Config>,
    hub: Hub,
    orphans_pool: Arc<AsyncRwLock<OrphanBlocksPool>>,
    shared_block_requests: Arc<Mutex<HashSet<Hash>>>,
    transactions_spread: Arc<AsyncRwLock<TransactionsSpread>>,
    shared_transaction_requests: Arc<Mutex<HashSet<TransactionId>>>,
    is_ibd_running: Arc<AtomicBool>,
    pub address_manager: Arc<Mutex<AddressManager>>,
    mining_manager: Arc<MiningManager>,
    notification_root: Arc<ConsensusNotificationRoot>,
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
        mining_manager: Arc<MiningManager>,
        notification_root: Arc<ConsensusNotificationRoot>,
    ) -> Self {
        let hub = Hub::new();
        Self {
            inner: Arc::new(FlowContextInner {
                node_id: Uuid::new_v4(),
                consensus_manager,
                config,
                orphans_pool: Arc::new(AsyncRwLock::new(OrphanBlocksPool::new(MAX_ORPHANS))),
                shared_block_requests: Arc::new(Mutex::new(HashSet::new())),
                transactions_spread: Arc::new(AsyncRwLock::new(TransactionsSpread::new(hub.clone()))),
                shared_transaction_requests: Arc::new(Mutex::new(HashSet::new())),
                is_ibd_running: Arc::new(AtomicBool::default()),
                hub,
                address_manager,
                mining_manager,
                notification_root,
            }),
        }
    }

    pub fn consensus(&self) -> ConsensusInstance {
        self.consensus_manager.consensus()
    }

    pub fn hub(&self) -> &Hub {
        &self.hub
    }

    pub fn mining_manager(&self) -> &MiningManager {
        &self.mining_manager
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

    pub async fn get_orphan_roots(&self, consensus: &dyn ConsensusApi, orphan: Hash) -> Option<Vec<Hash>> {
        self.orphans_pool.read().await.get_orphan_roots(consensus, orphan)
    }

    pub async fn unorphan_blocks(&self, consensus: &dyn ConsensusApi, root: Hash) -> Vec<Block> {
        self.orphans_pool.write().await.unorphan_blocks(consensus, root).await
    }

    /// Adds the given block to the DAG and propagates it.
    pub async fn add_block(&self, consensus: &dyn ConsensusApi, block: Block) -> Result<(), ProtocolError> {
        if block.transactions.is_empty() {
            return Err(RuleError::NoTransactions)?;
        }
        let hash = block.hash();
        if let Err(err) = self.consensus().session().await.validate_and_insert_block(block.clone(), true).await {
            warn!("Validation failed for block {}: {}", hash, err);
            return Err(err)?;
        }
        self.on_new_block_template().await?;
        self.on_new_block(consensus, block).await?;
        self.hub.broadcast(make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(hash.into()) })).await;
        Ok(())
    }

    /// Updates the mempool after a new block arrival, relays newly unorphaned transactions
    /// and possibly rebroadcast manually added transactions when not in IBD.
    ///
    /// _GO-KASPAD: OnNewBlock + broadcastTransactionsAfterBlockAdded_
    pub async fn on_new_block(&self, consensus: &dyn ConsensusApi, block: Block) -> Result<(), ProtocolError> {
        let hash = block.hash();
        let blocks = self.unorphan_blocks(consensus, hash).await;
        // Use a ProcessQueue so we get rid of duplicates
        let mut transactions_to_broadcast = ProcessQueue::new();
        for block in once(block).chain(blocks.into_iter()) {
            transactions_to_broadcast.enqueue_chunk(
                self.mining_manager().handle_new_block_transactions(consensus, &block.transactions)?.iter().map(|x| x.id()),
            );
        }

        // Don't relay transactions when in IBD
        if self.is_ibd_running() {
            return Ok(());
        }

        if self.should_rebroadcast_transactions().await {
            transactions_to_broadcast
                .enqueue_chunk(self.mining_manager().revalidate_high_priority_transactions(consensus)?.into_iter());
        }

        self.broadcast_transactions(transactions_to_broadcast).await
    }

    /// Notifies that a new block template is available for miners.
    pub async fn on_new_block_template(&self) -> Result<(), ProtocolError> {
        // Clear current template cache
        self.mining_manager().clear_block_template();
        // TODO: better handle notification errors
        self.notification_root
            .notify(Notification::NewBlockTemplate(NewBlockTemplateNotification {}))
            .map_err(|_| ProtocolError::Other("Notification error"))?;
        Ok(())
    }

    /// Notifies that a transaction has been added to the mempool.
    pub async fn on_transaction_added_to_mempool(&self) {
        // TODO: call a handler function or a predefined registered service
    }

    pub async fn add_transaction(
        &self,
        consensus: &dyn ConsensusApi,
        transaction: Transaction,
        orphan: Orphan,
    ) -> Result<(), ProtocolError> {
        let accepted_transactions =
            self.mining_manager().validate_and_insert_transaction(consensus, transaction, Priority::High, orphan)?;
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
}

#[async_trait]
impl ConnectionInitializer for FlowContext {
    async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ProtocolError> {
        // Build the handshake object and subscribe to handshake messages
        let mut handshake = KaspadHandshake::new(&router);

        // We start the router receive loop only after we registered to handshake routes
        router.start();

        let network_name = self.config.network_name();

        // Build the local version message
        // TODO: full and accurate version info
        let self_version_message = pb::VersionMessage {
            protocol_version: PROTOCOL_VERSION,
            services: 0, // TODO: get number of live services
            timestamp: unix_now() as i64,
            address: None, // TODO
            id: Vec::from(self.node_id.as_ref()),
            user_agent: String::new(), // TODO
            disable_relay_tx: false,   // TODO: config/cmd
            subnetwork_id: None,       // Subnets are not currently supported
            network: network_name.clone(),
        };

        // Perform the handshake
        let peer_version_message = handshake.handshake(self_version_message).await?;

        if peer_version_message.network != network_name {
            return Err(ProtocolError::WrongNetwork(network_name, peer_version_message.network));
        }

        debug!("protocol versions - self: {}, peer: {}", PROTOCOL_VERSION, peer_version_message.protocol_version);

        // Register all flows according to version
        let flows = match peer_version_message.protocol_version {
            PROTOCOL_VERSION => v5::register(self.clone(), router.clone()),
            // TODO: different errors for obsolete (low version) vs unknown (high)
            v => return Err(ProtocolError::VersionMismatch(PROTOCOL_VERSION, v)),
        };

        // Send and receive the ready signal
        handshake.exchange_ready_messages().await?;

        info!("Registering p2p flows for peer {} for protocol version {}", router, peer_version_message.protocol_version);

        // Launch all flows. Note we launch only after the ready signal was exchanged
        for flow in flows {
            flow.launch();
        }

        if router.is_outbound() {
            self.address_manager.lock().add_address(router.net_address().into());
        }

        // Note: we deliberately do not hold the handshake in memory so at this point receivers for handshake subscriptions
        // are dropped, hence effectively unsubscribing from these messages. This means that if the peer re-sends them
        // it is considered a protocol error and the connection will disconnect

        Ok(())
    }
}
