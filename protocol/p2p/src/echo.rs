use crate::{
    adaptor::{ConnectionError, ConnectionInitializer},
    pb::{self, KaspadMessage},
    KaspadMessagePayloadType, Router,
};
use kaspa_core::{debug, info, trace, warn};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver as MpscReceiver;
use tonic::async_trait;
use uuid::Uuid;

/// An example flow, echoing all messages back to the network
pub struct EchoFlow {
    receiver: MpscReceiver<KaspadMessage>,
    router: Arc<Router>,
}

impl EchoFlow {
    pub async fn register(router: Arc<Router>) {
        // Subscribe to messages
        trace!("EchoFlow, subscribe to all p2p messages");
        let receiver = router.subscribe(vec![
            KaspadMessagePayloadType::Addresses,
            KaspadMessagePayloadType::Block,
            KaspadMessagePayloadType::Transaction,
            KaspadMessagePayloadType::BlockLocator,
            KaspadMessagePayloadType::RequestAddresses,
            KaspadMessagePayloadType::RequestRelayBlocks,
            KaspadMessagePayloadType::RequestTransactions,
            KaspadMessagePayloadType::IbdBlock,
            KaspadMessagePayloadType::InvRelayBlock,
            KaspadMessagePayloadType::InvTransactions,
            KaspadMessagePayloadType::Ping,
            KaspadMessagePayloadType::Pong,
            // KaspadMessagePayloadType::Verack,
            // KaspadMessagePayloadType::Version,
            // KaspadMessagePayloadType::Ready,
            KaspadMessagePayloadType::TransactionNotFound,
            KaspadMessagePayloadType::Reject,
            KaspadMessagePayloadType::PruningPointUtxoSetChunk,
            KaspadMessagePayloadType::RequestIbdBlocks,
            KaspadMessagePayloadType::UnexpectedPruningPoint,
            KaspadMessagePayloadType::IbdBlockLocator,
            KaspadMessagePayloadType::IbdBlockLocatorHighestHash,
            KaspadMessagePayloadType::RequestNextPruningPointUtxoSetChunk,
            KaspadMessagePayloadType::DonePruningPointUtxoSetChunks,
            KaspadMessagePayloadType::IbdBlockLocatorHighestHashNotFound,
            KaspadMessagePayloadType::BlockWithTrustedData,
            KaspadMessagePayloadType::DoneBlocksWithTrustedData,
            KaspadMessagePayloadType::RequestPruningPointAndItsAnticone,
            KaspadMessagePayloadType::BlockHeaders,
            KaspadMessagePayloadType::RequestNextHeaders,
            KaspadMessagePayloadType::DoneHeaders,
            KaspadMessagePayloadType::RequestPruningPointUtxoSet,
            KaspadMessagePayloadType::RequestHeaders,
            KaspadMessagePayloadType::RequestBlockLocator,
            KaspadMessagePayloadType::PruningPoints,
            KaspadMessagePayloadType::RequestPruningPointProof,
            KaspadMessagePayloadType::PruningPointProof,
            KaspadMessagePayloadType::BlockWithTrustedDataV4,
            KaspadMessagePayloadType::TrustedData,
            KaspadMessagePayloadType::RequestIbdChainBlockLocator,
            KaspadMessagePayloadType::IbdChainBlockLocator,
            KaspadMessagePayloadType::RequestAnticone,
            KaspadMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks,
        ]);
        let mut echo_flow = EchoFlow { router, receiver };
        debug!("EchoFlow, start app-layer receiving loop");
        tokio::spawn(async move {
            debug!("EchoFlow, start message dispatching loop");
            while let Some(msg) = echo_flow.receiver.recv().await {
                if !(echo_flow.call(msg).await) {
                    warn!("EchoFlow, receive loop - call failed");
                    break;
                }
            }
            debug!("EchoFlow, existing message dispatch loop");
        });
    }

    /// This an example `call` to make a point that only inside this call the code starts to be
    /// maybe not generic
    async fn call(&self, msg: pb::KaspadMessage) -> bool {
        // echo
        trace!("EchoFlow, got message:{:?}", msg);
        self.router.route_to_network(msg).await
    }
}

/// An example initializer, performing handshake and registering a simple echo flow
#[derive(Default)]
pub struct EchoFlowInitializer {}

#[inline]
fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

impl EchoFlowInitializer {
    pub fn new() -> Self {
        EchoFlowInitializer {}
    }

    async fn receive_version_flow(&self, router: &Arc<Router>, mut receiver: MpscReceiver<KaspadMessage>) {
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

    async fn send_version_flow(&self, router: &Arc<Router>, mut receiver: MpscReceiver<KaspadMessage>) {
        info!("starting send version flow");
        let version_message = pb::VersionMessage {
            protocol_version: 5,
            services: 0,
            timestamp: unix_now(),
            address: None,
            id: Vec::from(Uuid::new_v4().as_ref()),
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

    async fn ready_flow(&self, router: &Arc<Router>, mut receiver: MpscReceiver<KaspadMessage>) {
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
        version_receiver: MpscReceiver<KaspadMessage>,
        verack_receiver: MpscReceiver<KaspadMessage>,
        ready_receiver: MpscReceiver<KaspadMessage>,
    ) {
        // Run both send and receive flows concurrently
        tokio::join!(self.send_version_flow(router, verack_receiver), self.receive_version_flow(router, version_receiver));
        self.ready_flow(router, ready_receiver).await;
    }
}

#[async_trait]
impl ConnectionInitializer for EchoFlowInitializer {
    async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ConnectionError> {
        //
        // Example code to illustrate kaspa P2P handshaking
        //

        // Subscribe to handshake messages
        let version_receiver = router.subscribe(vec![KaspadMessagePayloadType::Version]);
        let verack_receiver = router.subscribe(vec![KaspadMessagePayloadType::Verack]);
        let ready_receiver = router.subscribe(vec![KaspadMessagePayloadType::Ready]);

        // Start the router receive loop
        router.start();
        // Perform the handshake
        self.handshake(&router, version_receiver, verack_receiver, ready_receiver).await;

        // Subscribe to remaining messages
        EchoFlow::register(router.clone()).await;
        Ok(())
    }
}
