use async_trait::async_trait;
use consensus_core::api::DynConsensus;
use kaspa_core::{info, trace};
use p2p_lib::infra::{KaspadMessagePayloadEnumU8, Router, RouterApi};
use p2p_lib::pb::{self, KaspadMessage};
use p2p_lib::registry::{Flow, FlowRegistryApi, FlowTxTerminateChannelType};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use uuid::Uuid;

use crate::v5::echo::EchoFlow;

pub struct FlowContext {
    /// For now, directly hold consensus
    pub consensus: DynConsensus,
}

impl FlowContext {
    pub fn new(consensus: DynConsensus) -> Self {
        Self { consensus }
    }

    pub fn get_consensus(&self) -> DynConsensus {
        self.consensus.clone()
    }
}

#[inline]
fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

impl FlowContext {
    async fn receive_version_flow(&self, router: &Arc<Router>, mut receiver: Receiver<KaspadMessage>) {
        info!("starting receive version flow");
        if let Some(msg) = receiver.recv().await {
            if let pb::kaspad_message::Payload::Version(version_message) = msg.payload.unwrap() {
                info!("accepted version massage: {version_message:?}");
                let verack_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Verack(pb::VerackMessage {})) };
                router.route_to_network(verack_message).await;
                return;
            }
        }
        panic!()
    }

    async fn send_version_flow(&self, router: &Arc<Router>, mut receiver: Receiver<KaspadMessage>) {
        info!("starting send version flow");
        // TODO: full and accurate version info
        let version_message = pb::VersionMessage {
            protocol_version: 5,
            services: 0,
            timestamp: unix_now(),
            address: None,
            id: Vec::from(Uuid::new_v4().as_ref()), // TODO
            user_agent: String::new(),
            disable_relay_tx: true,
            subnetwork_id: None,
            network: "kaspa-mainnet".to_string(),
        };
        let version_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Version(version_message)) };
        router.route_to_network(version_message).await;

        if let Some(msg) = receiver.recv().await {
            if let pb::kaspad_message::Payload::Verack(verack_message) = msg.payload.unwrap() {
                info!("accepted verack_message: {verack_message:?}");
                return;
            }
        }
        panic!()
    }

    async fn ready_flow(&self, router: &Arc<Router>, mut receiver: Receiver<KaspadMessage>) {
        info!("starting ready flow");
        let ready_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Ready(pb::ReadyMessage {})) };
        router.route_to_network(ready_message).await;
        if let Some(msg) = receiver.recv().await {
            if let pb::kaspad_message::Payload::Ready(ready_message) = msg.payload.unwrap() {
                info!("accepted ready message: {ready_message:?}");
                return;
            }
        }
        panic!()
    }

    async fn handshake(
        &self,
        router: &Arc<Router>,
        version_receiver: Receiver<KaspadMessage>,
        verack_receiver: Receiver<KaspadMessage>,
        ready_receiver: Receiver<KaspadMessage>,
    ) {
        // Run both send and receive flows concurrently
        tokio::join!(self.send_version_flow(router, verack_receiver), self.receive_version_flow(router, version_receiver));
        self.ready_flow(router, ready_receiver).await;
    }
}

#[async_trait]
impl FlowRegistryApi for FlowContext {
    async fn initialize_flows(&self, router: Arc<Router>) -> Vec<(Uuid, FlowTxTerminateChannelType)> {
        // Subscribe to handshake messages
        let version_receiver = router.subscribe_to(vec![KaspadMessagePayloadEnumU8::Version]);
        let verack_receiver = router.subscribe_to(vec![KaspadMessagePayloadEnumU8::Verack]);
        let ready_receiver = router.subscribe_to(vec![KaspadMessagePayloadEnumU8::Ready]);

        // Subscribe to remaining messages and finalize (finalize will reroute all messages into flows)
        // TODO: register to all kaspa P2P flows here
        let echo_terminate = EchoFlow::new(router.clone()).await;
        trace!("finalizing flow subscriptions");
        router.finalize().await;

        // Perform the initial handshake.
        // TODO: error handling.
        self.handshake(&router, version_receiver, verack_receiver, ready_receiver).await;

        vec![echo_terminate]
    }
}
