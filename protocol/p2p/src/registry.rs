use crate::infra;
use crate::infra::{KaspadMessagePayloadEnumU8, Router, RouterApi};
use crate::pb::{self, KaspadMessage};
use kaspa_core::{debug, info, trace, warn};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use tonic::async_trait;
use uuid::Uuid;

pub type FlowTxTerminateChannelType = tokio::sync::oneshot::Sender<()>;
pub type FlowRxTerminateChannelType = tokio::sync::oneshot::Receiver<()>;

/// The main entrypoint for external usage of the P2P library. An impl of this trait is expected on
/// P2P server initialization and will be called on each new P2P connection with a corresponding dedicated router
#[async_trait]
pub trait FlowRegistryApi: Sync + Send {
    async fn initialize_flows(&self, router: Arc<infra::Router>) -> Vec<(Uuid, FlowTxTerminateChannelType)>;
}

/// An example Flow trait. Registry implementors can deviate from this structure.
#[async_trait]
pub trait Flow {
    #[allow(clippy::new_ret_no_self)]
    async fn new(router: Arc<infra::Router>) -> (Uuid, FlowTxTerminateChannelType);
    async fn call(&self, msg: pb::KaspadMessage) -> bool;
}

/// An example flow, echoing all messages back to the network
#[allow(dead_code)]
pub struct EchoFlow {
    receiver: infra::RouterRxChannelType,
    router: Arc<infra::Router>,
    terminate: FlowRxTerminateChannelType,
}

#[async_trait]
impl Flow for EchoFlow {
    async fn new(router: Arc<Router>) -> (Uuid, FlowTxTerminateChannelType) {
        // [0] - subscribe to messages
        trace!("EchoFlow, subscribe to all p2p messages");
        let receiver = router.subscribe_to(vec![
            KaspadMessagePayloadEnumU8::Addresses,
            KaspadMessagePayloadEnumU8::Block,
            KaspadMessagePayloadEnumU8::Transaction,
            KaspadMessagePayloadEnumU8::BlockLocator,
            KaspadMessagePayloadEnumU8::RequestAddresses,
            KaspadMessagePayloadEnumU8::RequestRelayBlocks,
            KaspadMessagePayloadEnumU8::RequestTransactions,
            KaspadMessagePayloadEnumU8::IbdBlock,
            KaspadMessagePayloadEnumU8::InvRelayBlock,
            KaspadMessagePayloadEnumU8::InvTransactions,
            KaspadMessagePayloadEnumU8::Ping,
            KaspadMessagePayloadEnumU8::Pong,
            // The below message types are registered during handshake
            // KaspadMessagePayloadEnumU8::Verack,
            // KaspadMessagePayloadEnumU8::Version,
            // KaspadMessagePayloadEnumU8::Ready,
            KaspadMessagePayloadEnumU8::TransactionNotFound,
            KaspadMessagePayloadEnumU8::Reject,
            KaspadMessagePayloadEnumU8::PruningPointUtxoSetChunk,
            KaspadMessagePayloadEnumU8::RequestIbdBlocks,
            KaspadMessagePayloadEnumU8::UnexpectedPruningPoint,
            KaspadMessagePayloadEnumU8::IbdBlockLocator,
            KaspadMessagePayloadEnumU8::IbdBlockLocatorHighestHash,
            KaspadMessagePayloadEnumU8::RequestNextPruningPointUtxoSetChunk,
            KaspadMessagePayloadEnumU8::DonePruningPointUtxoSetChunks,
            KaspadMessagePayloadEnumU8::IbdBlockLocatorHighestHashNotFound,
            KaspadMessagePayloadEnumU8::BlockWithTrustedData,
            KaspadMessagePayloadEnumU8::DoneBlocksWithTrustedData,
            KaspadMessagePayloadEnumU8::RequestPruningPointAndItsAnticone,
            KaspadMessagePayloadEnumU8::BlockHeaders,
            KaspadMessagePayloadEnumU8::RequestNextHeaders,
            KaspadMessagePayloadEnumU8::DoneHeaders,
            KaspadMessagePayloadEnumU8::RequestPruningPointUtxoSet,
            KaspadMessagePayloadEnumU8::RequestHeaders,
            KaspadMessagePayloadEnumU8::RequestBlockLocator,
            KaspadMessagePayloadEnumU8::PruningPoints,
            KaspadMessagePayloadEnumU8::RequestPruningPointProof,
            KaspadMessagePayloadEnumU8::PruningPointProof,
            KaspadMessagePayloadEnumU8::BlockWithTrustedDataV4,
            KaspadMessagePayloadEnumU8::TrustedData,
            KaspadMessagePayloadEnumU8::RequestIbdChainBlockLocator,
            KaspadMessagePayloadEnumU8::IbdChainBlockLocator,
            KaspadMessagePayloadEnumU8::RequestAnticone,
            KaspadMessagePayloadEnumU8::RequestNextPruningPointAndItsAnticoneBlocks,
        ]);
        // reroute....()
        // [1] - close default channel & reroute
        // in case we still didn't registered all flows, we will use reroute_to_flow() call
        // and only after all flows are registered, reroute_to_flow_and_close_default_route() must be used
        trace!("EchoFlow, finalize subscription");
        router.finalize().await;
        // [2] - terminate channel
        let (term_tx, term_rx) = tokio::sync::oneshot::channel();
        // [3] - create object
        let mut echo_flow = EchoFlow { router, receiver, terminate: term_rx };
        // [4] - spawn on echo_flow object
        trace!("EchoFlow, start app-layer receiving loop");
        tokio::spawn(async move {
            debug!("EchoFlow, start message dispatching loop");
            loop {
                tokio::select! {
                    // [4.0] - receive
                    Some(msg) = echo_flow.receiver.recv() => {
                        if !(echo_flow.call(msg).await) {
                            warn!("EchoFlow, receive loop - call failed");
                            break;
                        }
                    }
                    // [4.1] - terminate
                    _ = &mut echo_flow.terminate => {
                        debug!("EchoFlow, terminate was requested");
                        break;
                    }
                    // [4.2] - terminate is recv return error for example
                    else => {
                        debug!("EchoFlow - strange case");
                        break
                    }
                };
            }
        });
        // [5] - return management channel to terminate this flow with term_tx.send(...)
        debug!("EchoFlow, returning terminate control to the caller");
        (Uuid::new_v4(), term_tx)
    }
    // this an example `call` to make a point that only inside this call the code starts to be
    // maybe not generic
    async fn call(&self, msg: pb::KaspadMessage) -> bool {
        // echo
        trace!("EchoFlow, got message:{:?}", msg);
        self.router.route_to_network(msg).await
    }
}

/// An example registry, performing handshake and registering a simple echo flow
#[derive(Default)]
pub struct EchoFlowRegistry {}

#[inline]
fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

impl EchoFlowRegistry {
    pub fn new() -> Self {
        EchoFlowRegistry {}
    }

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
impl FlowRegistryApi for EchoFlowRegistry {
    async fn initialize_flows(&self, router: Arc<Router>) -> Vec<(Uuid, FlowTxTerminateChannelType)> {
        //
        // Example code to illustrate kaspa P2P handshaking
        //

        // Subscribe to handshake messages
        let version_receiver = router.subscribe_to(vec![KaspadMessagePayloadEnumU8::Version]);
        let verack_receiver = router.subscribe_to(vec![KaspadMessagePayloadEnumU8::Verack]);
        let ready_receiver = router.subscribe_to(vec![KaspadMessagePayloadEnumU8::Ready]);

        // Subscribe to remaining messages and finalize (finalize will reroute all messages into flows)
        let echo_terminate = EchoFlow::new(router.clone()).await;

        // Perform the handshake
        self.handshake(&router, version_receiver, verack_receiver, ready_receiver).await;

        vec![echo_terminate]
    }
}
