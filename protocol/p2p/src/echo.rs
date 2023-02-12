use crate::{
    adaptor::{ConnectionError, ConnectionInitializer},
    handshake::KaspadHandshake,
    pb::{self, KaspadMessage, VersionMessage},
    KaspadMessagePayloadType, Router,
};
use kaspa_core::{debug, trace, warn};
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

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

fn build_dummy_version_message() -> VersionMessage {
    pb::VersionMessage {
        protocol_version: 5,
        services: 0,
        timestamp: unix_now(),
        address: None,
        id: Vec::from(Uuid::new_v4().as_ref()),
        user_agent: String::new(),
        disable_relay_tx: false,
        subnetwork_id: None,
        network: "kaspa-mainnet".to_string(),
    }
}

impl EchoFlowInitializer {
    pub fn new() -> Self {
        EchoFlowInitializer {}
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

        // We start the router receive loop only after we registered to handshake routes
        router.start();

        // Build the local version message
        let self_version_message = build_dummy_version_message();

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
