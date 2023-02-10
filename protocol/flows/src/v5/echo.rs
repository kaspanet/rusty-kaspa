use kaspa_core::{debug, trace, warn};
use p2p_lib::{
    pb::{self, KaspadMessage},
    KaspadMessagePayloadType, Router,
};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver as MpscReceiver;

/// An example flow, echoing all messages back to the network
pub struct EchoFlow {
    receiver: MpscReceiver<KaspadMessage>,
    router: Arc<Router>,
}

impl EchoFlow {
    pub async fn register(router: Arc<Router>) {
        // Subscribe to messages
        trace!("EchoFlow, subscribe to all p2p messages");
        let receiver = router
            .subscribe(vec![
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
            ])
            .await;
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
