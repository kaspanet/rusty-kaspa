use crate::kaspa_grpc;
use crate::kaspa_grpc::{KaspadMessagePayloadEnumU8, Router, RouterApi};
use crate::pb;
use kaspa_core::warn;
use log::{debug, trace};
use std::sync::Arc;
use tonic::async_trait;

pub type FlowTxTerminateChannelType = tokio::sync::oneshot::Sender<()>;
pub type FlowRxTerminateChannelType = tokio::sync::oneshot::Receiver<()>;

#[async_trait]
pub trait Flow {
    #[allow(clippy::new_ret_no_self)]
    async fn new(router: std::sync::Arc<kaspa_grpc::Router>) -> FlowTxTerminateChannelType;
    async fn call(&self, msg: pb::KaspadMessage) -> bool;
}
#[allow(dead_code)]
pub struct EchoFlow {
    receiver: kaspa_grpc::RouterRxChannelType,
    router: std::sync::Arc<kaspa_grpc::Router>,
    terminate: FlowRxTerminateChannelType,
}

#[async_trait]
impl Flow for EchoFlow {
    async fn new(router: std::sync::Arc<Router>) -> FlowTxTerminateChannelType {
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
            KaspadMessagePayloadEnumU8::Verack,
            KaspadMessagePayloadEnumU8::Version,
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
            KaspadMessagePayloadEnumU8::Ready,
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
        trace!("EchoFlow, finilize subscription");
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
        term_tx
    }
    // this an example `call` to make a point that only inside this call the code starts to be
    // maybe not generic
    async fn call(&self, msg: pb::KaspadMessage) -> bool {
        // echo
        trace!("EchoFlow, got message:{:?}", msg);
        self.router.route_to_network(msg).await
    }
}

#[async_trait]
pub trait FlowRegistryApi {
    async fn initialize_flow(router: std::sync::Arc<kaspa_grpc::Router>) -> FlowTxTerminateChannelType;
}

pub struct FlowRegistry {}

#[async_trait]
impl FlowRegistryApi for FlowRegistry {
    async fn initialize_flow(router: Arc<Router>) -> FlowTxTerminateChannelType {
        EchoFlow::new(router).await
    }
}
