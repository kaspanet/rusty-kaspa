use self::{
    ibd::IbdFlow,
    ping::{ReceivePingsFlow, SendPingsFlow},
};
use crate::ctx::FlowContext;
use kaspa_core::{debug, warn};
use p2p_lib::{
    make_message,
    pb::{kaspad_message::Payload as KaspadMessagePayload, AddressesMessage},
    KaspadMessagePayloadType, Router,
};
use std::sync::Arc;

mod ibd;
mod ping;

pub fn register(ctx: FlowContext, router: Arc<Router>) {
    let ibd_incoming_route = router.subscribe(vec![
        KaspadMessagePayloadType::BlockHeaders,
        KaspadMessagePayloadType::DoneHeaders,
        KaspadMessagePayloadType::IbdBlockLocatorHighestHash,
        KaspadMessagePayloadType::IbdBlockLocatorHighestHashNotFound,
        KaspadMessagePayloadType::BlockWithTrustedDataV4,
        KaspadMessagePayloadType::DoneBlocksWithTrustedData,
        KaspadMessagePayloadType::IbdChainBlockLocator,
        KaspadMessagePayloadType::IbdBlock,
        KaspadMessagePayloadType::TrustedData,
        KaspadMessagePayloadType::PruningPoints,
        KaspadMessagePayloadType::PruningPointProof,
        KaspadMessagePayloadType::UnexpectedPruningPoint,
        KaspadMessagePayloadType::PruningPointUtxoSetChunk,
        KaspadMessagePayloadType::DonePruningPointUtxoSetChunks,
    ]);

    // TODO: generalize flow registration into a pattern
    let mut ibd_flow = IbdFlow::new(ctx.clone(), router.clone(), ibd_incoming_route);
    tokio::spawn(async move {
        let res = ibd_flow.start().await;
        if let Err(err) = res {
            warn!("IBD flow error: {}, disconnecting from peer.", err); // TODO: imp complete error handler with net-connection peer info etc
            ibd_flow.router.close().await;
        }
    });

    let mut receive_pings_flow =
        ReceivePingsFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![KaspadMessagePayloadType::Ping]));
    tokio::spawn(async move {
        let res = receive_pings_flow.start().await;
        if let Err(err) = res {
            warn!("Receive pings flow error: {}, disconnecting from peer.", err); // TODO: imp complete error handler with net-connection peer info etc
            receive_pings_flow.router.close().await;
        }
    });

    let mut send_pings_flow = SendPingsFlow::new(ctx, Arc::downgrade(&router), router.subscribe(vec![KaspadMessagePayloadType::Pong]));
    tokio::spawn(async move {
        let res = send_pings_flow.start().await;
        if let Err(err) = res {
            warn!("Send pings flow error: {}, disconnecting from peer.", err); // TODO: imp complete error handler with net-connection peer info etc
            if let Some(router) = send_pings_flow.router.upgrade() {
                router.close().await;
            }
        }
    });

    // TEMP: subscribe to remaining messages and ignore them
    // NOTE: as flows are implemented, the below types should be all commented out
    let mut unimplemented_messages_route = router.subscribe(vec![
        KaspadMessagePayloadType::Addresses,
        KaspadMessagePayloadType::Block,
        KaspadMessagePayloadType::Transaction,
        KaspadMessagePayloadType::BlockLocator,
        KaspadMessagePayloadType::RequestAddresses,
        KaspadMessagePayloadType::RequestRelayBlocks,
        KaspadMessagePayloadType::RequestTransactions,
        // KaspadMessagePayloadType::IbdBlock,
        KaspadMessagePayloadType::InvRelayBlock,
        KaspadMessagePayloadType::InvTransactions,
        // KaspadMessagePayloadType::Ping,
        // KaspadMessagePayloadType::Pong,
        // KaspadMessagePayloadType::Verack,
        // KaspadMessagePayloadType::Version,
        // KaspadMessagePayloadType::Ready,
        KaspadMessagePayloadType::TransactionNotFound,
        KaspadMessagePayloadType::Reject,
        // KaspadMessagePayloadType::PruningPointUtxoSetChunk,
        KaspadMessagePayloadType::RequestIbdBlocks,
        // KaspadMessagePayloadType::UnexpectedPruningPoint,
        KaspadMessagePayloadType::IbdBlockLocator,
        // KaspadMessagePayloadType::IbdBlockLocatorHighestHash,
        KaspadMessagePayloadType::RequestNextPruningPointUtxoSetChunk,
        // KaspadMessagePayloadType::DonePruningPointUtxoSetChunks,
        // KaspadMessagePayloadType::IbdBlockLocatorHighestHashNotFound,
        KaspadMessagePayloadType::BlockWithTrustedData,
        // KaspadMessagePayloadType::DoneBlocksWithTrustedData,
        KaspadMessagePayloadType::RequestPruningPointAndItsAnticone,
        // KaspadMessagePayloadType::BlockHeaders,
        KaspadMessagePayloadType::RequestNextHeaders,
        // KaspadMessagePayloadType::DoneHeaders,
        KaspadMessagePayloadType::RequestPruningPointUtxoSet,
        KaspadMessagePayloadType::RequestHeaders,
        KaspadMessagePayloadType::RequestBlockLocator,
        // KaspadMessagePayloadType::PruningPoints,
        KaspadMessagePayloadType::RequestPruningPointProof,
        // KaspadMessagePayloadType::PruningPointProof,
        // KaspadMessagePayloadType::BlockWithTrustedDataV4,
        // KaspadMessagePayloadType::TrustedData,
        KaspadMessagePayloadType::RequestIbdChainBlockLocator,
        // KaspadMessagePayloadType::IbdChainBlockLocator,
        KaspadMessagePayloadType::RequestAnticone,
        KaspadMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks,
    ]);

    tokio::spawn(async move {
        while let Some(msg) = unimplemented_messages_route.recv().await {
            // TEMP: responding to this request is required in order to keep the
            // connection live until we implement the send addresses flow
            if let Some(KaspadMessagePayload::RequestAddresses(_)) = msg.payload {
                debug!("P2P Flows, got request addresses message");
                let _ =
                    router.enqueue(make_message!(KaspadMessagePayload::Addresses, AddressesMessage { address_list: vec![] })).await;
            }
        }
    });
}
