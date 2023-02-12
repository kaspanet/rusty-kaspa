use async_trait::async_trait;
use consensus_core::api::DynConsensus;
use kaspa_core::debug;
use p2p_lib::echo::EchoFlow;
use p2p_lib::pb;
use p2p_lib::{ConnectionError, ConnectionInitializer, Router};
use p2p_lib::{KaspadHandshake, KaspadMessagePayloadType};
use std::sync::Arc;
use uuid::Uuid;

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
        debug!("protocol versions - self: {}, peer: {}", 5, peer_version_message.protocol_version);

        // Subscribe to remaining messages. In this example we simply subscribe to all messages with a single echo flow
        EchoFlow::register(router.clone()).await;

        // Send a ready signal
        handshake.ready_flow(&router, ready_receiver).await?;

        // Note: at this point receivers for handshake subscriptions
        // are dropped, thus effectively unsubscribing

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_core::debug;
    use p2p_lib::Adaptor;

    #[tokio::test]
    async fn test_handshake() {
        kaspa_core::log::try_init_logger("debug");

        let address1 = String::from("[::1]:50053");
        let adaptor1 = Adaptor::bidirectional(address1.clone(), Arc::new(FlowContext::new(None))).unwrap();

        let address2 = String::from("[::1]:50054");
        let adaptor2 = Adaptor::bidirectional(address2.clone(), Arc::new(FlowContext::new(None))).unwrap();

        // Initiate the connection from `adaptor1` (outbound) to `adaptor2` (inbound)
        // NOTE: a minimal scheme prefix `"://"` must be added for the client-side connect logic
        let peer2_id = adaptor1.connect_peer(String::from("://[::1]:50054")).await.expect("peer connection failed");

        // TODO: find a better mechanism to sync on handshake completion
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // For now assert the handshake by checking the peer exists (since peer is removed on handshake error)
        assert_eq!(adaptor1.get_active_peers().await.len(), 1, "handshake failed -- outbound peer is missing");
        assert_eq!(adaptor2.get_active_peers().await.len(), 1, "handshake failed -- inbound peer is missing");

        adaptor1.terminate(peer2_id).await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        assert_eq!(adaptor1.get_active_peers().await.len(), 0, "peer termination failed -- outbound peer was not removed");
        assert_eq!(adaptor2.get_active_peers().await.len(), 0, "peer termination failed -- inbound peer was not removed");

        adaptor1.close().await;
        adaptor2.close().await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        debug!("{} {}", Arc::strong_count(&adaptor1), Arc::strong_count(&adaptor2));
        assert_eq!(Arc::strong_count(&adaptor1), 1, "some adaptor resources did not cleanup");
        assert_eq!(Arc::strong_count(&adaptor2), 1, "some adaptor resources did not cleanup");

        drop(adaptor1);
        drop(adaptor2);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
