use crate::v5;
use async_trait::async_trait;
use consensus_core::api::DynConsensus;
use kaspa_core::debug;
use kaspa_core::time::unix_now;
use p2p_lib::pb;
use p2p_lib::{common::ProtocolError, ConnectionInitializer, KaspadHandshake, Router};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct FlowContext {
    pub consensus: DynConsensus,
}

impl FlowContext {
    pub fn new(consensus: DynConsensus) -> Self {
        Self { consensus }
    }

    pub fn consensus(&self) -> DynConsensus {
        self.consensus.clone()
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

        // Launch all flows. Note we launch only after the ready signal was exchanged
        for flow in flows {
            flow.launch();
        }

        // Note: we deliberately do not hold the handshake in memory so at this point receivers for handshake subscriptions
        // are dropped, hence effectively unsubscribing from these messages. This means that if the peer re-sends them
        // it is considered a protocol error and the connection will disconnect

        Ok(())
    }
}
