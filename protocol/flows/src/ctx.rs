use crate::v5;
use async_trait::async_trait;
use consensus_core::api::DynConsensus;
use kaspa_core::debug;
use p2p_lib::pb;
use p2p_lib::{ConnectionError, ConnectionInitializer, Router};
use p2p_lib::{KaspadHandshake, KaspadMessagePayloadType};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct FlowContext {
    /// For now, directly hold consensus
    pub consensus: Option<DynConsensus>,
}

impl FlowContext {
    pub fn new(consensus: Option<DynConsensus>) -> Self {
        Self { consensus }
    }

    pub fn consensus(&self) -> DynConsensus {
        self.consensus.clone().unwrap()
    }
}

#[inline]
fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

#[async_trait]
impl ConnectionInitializer for FlowContext {
    async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ConnectionError> {
        // Subscribe to handshake messages
        let version_receiver = router.subscribe(vec![KaspadMessagePayloadType::Version]);
        let verack_receiver = router.subscribe(vec![KaspadMessagePayloadType::Verack]);
        let ready_receiver = router.subscribe(vec![KaspadMessagePayloadType::Ready]);

        // We start the router receive loop only after we registered to handshake routes
        router.start();

        // Build the local version message
        // TODO: full and accurate version info
        let self_version_message = pb::VersionMessage {
            protocol_version: 5, // TODO: make a const
            services: 0,         // TODO: get number of live services
            timestamp: unix_now(),
            address: None,                          // TODO
            id: Vec::from(Uuid::new_v4().as_ref()), // TODO
            user_agent: String::new(),              // TODO
            disable_relay_tx: false,                // TODO: config/cmd?
            subnetwork_id: None,                    // Subnets are not currently supported
            network: "kaspa-mainnet".to_string(),   // TODO: get network from config
        };

        // Build the handshake object
        let handshake = KaspadHandshake::new();

        // Perform the handshake
        let peer_version_message = handshake.handshake(&router, version_receiver, verack_receiver, self_version_message).await?;

        // TODO: verify the versions are compatible
        debug!("protocol versions - self: {}, peer: {}", 5, peer_version_message.protocol_version);

        // Register all flows according to version
        let flows = match peer_version_message.protocol_version {
            5 => v5::register(self.clone(), router.clone()),
            _ => todo!(),
        };

        // Send and receive the ready signal
        handshake.ready_flow(&router, ready_receiver).await?;

        // Launch all flows. Note we launch only after the ready signal was exchanged
        for flow in flows {
            flow.launch();
        }

        // Note: at this point receivers for handshake subscriptions
        // are dropped, thus effectively unsubscribing from these messages, which means that if the peer re-sends them
        // it is considered a protocol error and the connection will disconnect

        Ok(())
    }
}
