use async_trait::async_trait;
use consensus_core::api::DynConsensus;
use kaspa_core::info;
use p2p_lib::pb::{self, kaspad_message::Payload, KaspadMessage, VersionMessage};
use p2p_lib::KaspadMessagePayloadType;
use p2p_lib::{ConnectionError, ConnectionInitializer, Router};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use uuid::Uuid;

use crate::common::FlowError;
use crate::recv_payload;
use crate::v5::echo::EchoFlow;

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

impl FlowContext {
    async fn receive_version_flow(
        &self,
        router: &Arc<Router>,
        mut receiver: Receiver<KaspadMessage>,
    ) -> Result<VersionMessage, FlowError> {
        info!("starting receive version flow");
        let version_message = recv_payload!(receiver, Payload::Version)?;
        info!("accepted version massage: {version_message:?}");
        let verack_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Verack(pb::VerackMessage {})) };
        router.route_to_network(verack_message).await;
        Ok(version_message)
    }

    async fn send_version_flow(&self, router: &Arc<Router>, mut receiver: Receiver<KaspadMessage>) -> Result<(), FlowError> {
        info!("starting send version flow");
        // TODO: full and accurate version info
        let version_message = pb::VersionMessage {
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
        let version_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Version(version_message)) };
        router.route_to_network(version_message).await;
        let verack_message = recv_payload!(receiver, Payload::Verack)?;
        info!("accepted verack_message: {verack_message:?}");
        Ok(())
    }

    async fn ready_flow(&self, router: &Arc<Router>, mut receiver: Receiver<KaspadMessage>) -> Result<(), FlowError> {
        info!("starting ready flow");
        let sent_ready_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Ready(pb::ReadyMessage {})) };
        router.route_to_network(sent_ready_message).await;
        let recv_ready_message = recv_payload!(receiver, Payload::Ready)?;
        info!("accepted ready message: {recv_ready_message:?}");
        Ok(())
    }

    async fn handshake(
        &self,
        router: &Arc<Router>,
        version_receiver: Receiver<KaspadMessage>,
        verack_receiver: Receiver<KaspadMessage>,
    ) -> Result<VersionMessage, FlowError> {
        // Run both send and receive flows concurrently -- this is critical in order to avoid a handshake deadlock
        let (send_res, recv_res) =
            tokio::join!(self.send_version_flow(router, verack_receiver), self.receive_version_flow(router, version_receiver));
        send_res?;
        recv_res
    }
}

#[async_trait]
impl ConnectionInitializer for FlowContext {
    async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ConnectionError> {
        // Subscribe to handshake messages
        let version_receiver = router.subscribe(vec![KaspadMessagePayloadType::Version]).await;
        let verack_receiver = router.subscribe(vec![KaspadMessagePayloadType::Verack]).await;
        let ready_receiver = router.subscribe(vec![KaspadMessagePayloadType::Ready]).await;

        // We start the router receive loop only after we registered to handshake routes
        router.start().await;
        // Perform the initial handshake
        let version_message = self.handshake(&router, version_receiver, verack_receiver).await?;
        info!("peer protocol version: {}", version_message.protocol_version);

        // Subscribe to remaining messages
        // TODO: register to all kaspa P2P flows here
        EchoFlow::register(router.clone()).await;

        // Send a ready signal
        self.ready_flow(&router, ready_receiver).await?;

        // Note: at this point receivers for handshake subscriptions
        // are dropped, thus effectively unsubscribing

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p2p_lib::Adaptor;

    #[tokio::test]
    async fn test_p2p_handshake() {
        kaspa_core::log::try_init_logger("debug");

        let address1 = String::from("[::1]:50053");
        let adaptor1 = Adaptor::bidirectional_connection(address1.clone(), Arc::new(FlowContext::new(None))).unwrap();

        let address2 = String::from("[::1]:50054");
        let adaptor2 = Adaptor::bidirectional_connection(address2.clone(), Arc::new(FlowContext::new(None))).unwrap();

        // Initiate the connection from `adaptor1` (outbound) to `adaptor2` (inbound)
        // NOTE: a minimal scheme prefix `"://"` must be added for the client-side connect logic
        let peer2_id = adaptor1.connect_peer(String::from("://[::1]:50054")).await.expect("peer connection failed");

        // TODO: find a better mechanism to sync on handshake completion
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;

        // For now assert the handshake by checking the peer exists (since peer is removed on handshake error)
        assert_eq!(adaptor1.get_active_peers().await.len(), 1, "handshake failed -- outbound peer is missing");
        assert_eq!(adaptor2.get_active_peers().await.len(), 1, "handshake failed -- inbound peer is missing");

        adaptor1.terminate(peer2_id).await;
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;

        assert_eq!(adaptor1.get_active_peers().await.len(), 0, "peer termination failed -- outbound peer was not removed");
        assert_eq!(adaptor2.get_active_peers().await.len(), 0, "peer termination failed -- inbound peer was not removed");
    }
}
