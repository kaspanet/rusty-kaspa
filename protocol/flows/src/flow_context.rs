use crate::flowcontext::orphans::{OrphanBlocksPool, MAX_ORPHANS};
use crate::v5;
use async_trait::async_trait;
use kaspa_addressmanager::AddressManager;
use kaspa_consensus_core::api::{ConsensusApi, DynConsensus};
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::Config;
use kaspa_core::time::unix_now;
use kaspa_core::{debug, info};
use kaspa_hashes::Hash;
use kaspa_p2p_lib::pb;
use kaspa_p2p_lib::{common::ProtocolError, ConnectionInitializer, KaspadHandshake, Router};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct FlowContext {
    pub consensus: DynConsensus,
    pub config: Config,
    orphans_pool: Arc<AsyncRwLock<OrphanBlocksPool<dyn ConsensusApi>>>,
    shared_block_requests: Arc<Mutex<HashSet<Hash>>>,
    is_ibd_running: Arc<AtomicBool>, // TODO: pass the context wrapped with Arc and avoid some of the internal ones
    pub amgr: Arc<Mutex<AddressManager>>,
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

pub struct BlockRequestScope<'a> {
    ctx: &'a FlowContext,
    req: Hash,
}

impl<'a> BlockRequestScope<'a> {
    pub fn new(ctx: &'a FlowContext, req: Hash) -> Self {
        Self { ctx, req }
    }
}

impl Drop for BlockRequestScope<'_> {
    fn drop(&mut self) {
        self.ctx.shared_block_requests.lock().remove(&self.req);
    }
}

impl FlowContext {
    pub fn new(consensus: DynConsensus, amgr: Arc<Mutex<AddressManager>>, config: &Config) -> Self {
        Self {
            consensus: consensus.clone(),
            config: config.clone(),
            orphans_pool: Arc::new(AsyncRwLock::new(OrphanBlocksPool::new(consensus, MAX_ORPHANS))),
            shared_block_requests: Arc::new(Mutex::new(HashSet::new())),
            is_ibd_running: Arc::new(AtomicBool::default()),
            amgr,
        }
    }

    pub fn consensus(&self) -> DynConsensus {
        self.consensus.clone()
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

    pub fn try_adding_block_request(&self, req: Hash) -> Option<BlockRequestScope> {
        if self.shared_block_requests.lock().insert(req) {
            Some(BlockRequestScope::new(self, req))
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
