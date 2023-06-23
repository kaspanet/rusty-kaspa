use self::{
    address::{ReceiveAddressesFlow, SendAddressesFlow},
    blockrelay::{flow::HandleRelayInvsFlow, handle_requests::HandleRelayBlockRequests},
    ibd::IbdFlow,
    ping::{ReceivePingsFlow, SendPingsFlow},
    request_anticone::HandleAnticoneRequests,
    request_block_locator::RequestBlockLocatorFlow,
    request_headers::RequestHeadersFlow,
    request_ibd_blocks::HandleIbdBlockRequests,
    request_ibd_chain_block_locator::RequestIbdChainBlockLocatorFlow,
    request_pp_proof::RequestPruningPointProofFlow,
    request_pruning_point_and_anticone::PruningPointAndItsAnticoneRequestsFlow,
    request_pruning_point_utxo_set::RequestPruningPointUtxoSetFlow,
    txrelay::flow::{RelayTransactionsFlow, RequestTransactionsFlow},
};
use crate::{flow_context::FlowContext, flow_trait::Flow};

use kaspa_p2p_lib::{KaspadMessagePayloadType, Router};
use std::sync::Arc;

mod address;
mod blockrelay;
mod ibd;
mod ping;
mod request_anticone;
mod request_block_locator;
mod request_headers;
mod request_ibd_blocks;
mod request_ibd_chain_block_locator;
mod request_pp_proof;
mod request_pruning_point_and_anticone;
mod request_pruning_point_utxo_set;
mod txrelay;

pub fn register(ctx: FlowContext, router: Arc<Router>) -> Vec<Box<dyn Flow>> {
    // IBD flow <-> invs flow channel requires no buffering hence the minimal size possible
    let (ibd_sender, relay_receiver) = tokio::sync::mpsc::channel(1);
    let flows: Vec<Box<dyn Flow>> = vec![
        Box::new(IbdFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
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
            ]),
            relay_receiver,
        )),
        Box::new(HandleRelayInvsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe_with_capacity(vec![KaspadMessagePayloadType::InvRelayBlock], ctx.block_invs_channel_size()),
            router.subscribe(vec![KaspadMessagePayloadType::Block, KaspadMessagePayloadType::BlockLocator]),
            ibd_sender,
        )),
        Box::new(HandleRelayBlockRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestRelayBlocks]),
        )),
        Box::new(ReceivePingsFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![KaspadMessagePayloadType::Ping]))),
        Box::new(SendPingsFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![KaspadMessagePayloadType::Pong]))),
        Box::new(RequestHeadersFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestHeaders, KaspadMessagePayloadType::RequestNextHeaders]),
        )),
        Box::new(RequestPruningPointProofFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestPruningPointProof]),
        )),
        Box::new(RequestIbdChainBlockLocatorFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestIbdChainBlockLocator]),
        )),
        Box::new(PruningPointAndItsAnticoneRequestsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                KaspadMessagePayloadType::RequestPruningPointAndItsAnticone,
                KaspadMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks,
            ]),
        )),
        Box::new(RequestPruningPointUtxoSetFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                KaspadMessagePayloadType::RequestPruningPointUtxoSet,
                KaspadMessagePayloadType::RequestNextPruningPointUtxoSetChunk,
            ]),
        )),
        Box::new(HandleIbdBlockRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestIbdBlocks]),
        )),
        Box::new(HandleAnticoneRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestAnticone]),
        )),
        Box::new(RelayTransactionsFlow::new(
            ctx.clone(),
            router.clone(),
            router
                .subscribe_with_capacity(vec![KaspadMessagePayloadType::InvTransactions], RelayTransactionsFlow::invs_channel_size()),
            router.subscribe_with_capacity(
                vec![KaspadMessagePayloadType::Transaction, KaspadMessagePayloadType::TransactionNotFound],
                RelayTransactionsFlow::txs_channel_size(),
            ),
        )),
        Box::new(RequestTransactionsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestTransactions]),
        )),
        Box::new(ReceiveAddressesFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![KaspadMessagePayloadType::Addresses]))),
        Box::new(SendAddressesFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestAddresses]),
        )),
        Box::new(RequestBlockLocatorFlow::new(
            ctx,
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestBlockLocator]),
        )),
    ];

    // The reject message is handled as a special case by the router
    // KaspadMessagePayloadType::Reject,

    // We do not register the below two messages since they are deprecated also in go-kaspa
    // KaspadMessagePayloadType::BlockWithTrustedData,
    // KaspadMessagePayloadType::IbdBlockLocator,

    flows
}
