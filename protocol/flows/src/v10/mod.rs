use self::{
    address::{ReceiveAddressesFlow, SendAddressesFlow},
    blockrelay::{flow::HandleRelayInvsFlow, handle_requests::HandleRelayBlockRequests},
    ping::{ReceivePingsFlow, SendPingsFlow},
    request_antipast::HandleAntipastRequests,
    request_block_bodies::HandleBlockBodyRequests,
    request_block_locator::RequestBlockLocatorFlow,
    request_headers::RequestHeadersFlow,
    request_ibd_blocks::HandleIbdBlockRequests,
    request_ibd_chain_block_locator::RequestIbdChainBlockLocatorFlow,
    request_pp_proof::RequestPruningPointProofFlow,
    request_pruning_point_and_anticone::PruningPointAndItsAnticoneRequestsFlow,
    request_pruning_point_utxo_set::RequestPruningPointUtxoSetFlow,
    txrelay::flow::{RelayTransactionsFlow, RequestTransactionsFlow},
};
pub(crate) mod address;
pub(crate) mod blockrelay;
pub(crate) mod ping;
pub(crate) mod request_antipast;
pub(crate) mod request_block_bodies;
pub(crate) mod request_block_locator;
pub(crate) mod request_headers;
pub(crate) mod request_ibd_blocks;
pub(crate) mod request_ibd_chain_block_locator;
pub(crate) mod request_pp_proof;
pub(crate) mod request_pruning_point_and_anticone;
pub(crate) mod request_pruning_point_smt_state;
pub(crate) mod request_pruning_point_utxo_set;
pub(crate) mod txrelay;
use request_pruning_point_smt_state::RequestPruningPointSmtStateFlow;

use crate::{flow_context::FlowContext, flow_trait::Flow, ibd::IbdFlow};
use aero_p2p_lib::{AerodMessagePayloadType, Router, SharedIncomingRoute};
use aero_utils::channel;
use std::sync::Arc;

pub fn register(ctx: FlowContext, router: Arc<Router>) -> Vec<Box<dyn Flow>> {
    let (ibd_sender, relay_receiver) = channel::job();
    let mut flows: Vec<Box<dyn Flow>> = vec![
        Box::new(IbdFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                AerodMessagePayloadType::BlockHeaders,
                AerodMessagePayloadType::DoneHeaders,
                AerodMessagePayloadType::IbdBlockLocatorHighestHash,
                AerodMessagePayloadType::IbdBlockLocatorHighestHashNotFound,
                AerodMessagePayloadType::BlockWithTrustedDataV4,
                AerodMessagePayloadType::DoneBlocksWithTrustedData,
                AerodMessagePayloadType::IbdChainBlockLocator,
                AerodMessagePayloadType::BlockBody,
                AerodMessagePayloadType::TrustedData,
                AerodMessagePayloadType::PruningPoints,
                AerodMessagePayloadType::PruningPointProof,
                AerodMessagePayloadType::UnexpectedPruningPoint,
                AerodMessagePayloadType::PruningPointUtxoSetChunk,
                AerodMessagePayloadType::DonePruningPointUtxoSetChunks,
                AerodMessagePayloadType::SmtMetadata,
                AerodMessagePayloadType::SmtLaneChunk,
            ]),
            relay_receiver,
        )),
        Box::new(HandleRelayBlockRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![AerodMessagePayloadType::RequestRelayBlocks]),
        )),
        Box::new(ReceivePingsFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![AerodMessagePayloadType::Ping]))),
        Box::new(SendPingsFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![AerodMessagePayloadType::Pong]))),
        Box::new(RequestHeadersFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![AerodMessagePayloadType::RequestHeaders, AerodMessagePayloadType::RequestNextHeaders]),
        )),
        Box::new(RequestPruningPointProofFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![AerodMessagePayloadType::RequestPruningPointProof]),
        )),
        Box::new(RequestIbdChainBlockLocatorFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![AerodMessagePayloadType::RequestIbdChainBlockLocator]),
        )),
        Box::new(PruningPointAndItsAnticoneRequestsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                AerodMessagePayloadType::RequestPruningPointAndItsAnticone,
                AerodMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks,
            ]),
        )),
        Box::new(RequestPruningPointUtxoSetFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                AerodMessagePayloadType::RequestPruningPointUtxoSet,
                AerodMessagePayloadType::RequestNextPruningPointUtxoSetChunk,
            ]),
        )),
        Box::new(RequestPruningPointSmtStateFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                AerodMessagePayloadType::RequestPruningPointSmtState,
                AerodMessagePayloadType::RequestNextPruningPointSmtChunk,
            ]),
        )),
        // The IBD client flow currently requests block bodies only. Keep serving full-block
        // requests in case a future IBD flow needs them again.
        Box::new(HandleIbdBlockRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![AerodMessagePayloadType::RequestIbdBlocks]),
        )),
        Box::new(HandleBlockBodyRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![AerodMessagePayloadType::RequestBlockBodies]),
        )),
        Box::new(HandleAntipastRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![AerodMessagePayloadType::RequestAntipast]),
        )),
        Box::new(RelayTransactionsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe_with_capacity(vec![AerodMessagePayloadType::InvTransactions], RelayTransactionsFlow::invs_channel_size()),
            router.subscribe_with_capacity(
                vec![AerodMessagePayloadType::Transaction, AerodMessagePayloadType::TransactionNotFound],
                RelayTransactionsFlow::txs_channel_size(),
            ),
        )),
        Box::new(RequestTransactionsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![AerodMessagePayloadType::RequestTransactions]),
        )),
        Box::new(ReceiveAddressesFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![AerodMessagePayloadType::Addresses]))),
        Box::new(SendAddressesFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![AerodMessagePayloadType::RequestAddresses]),
        )),
        Box::new(RequestBlockLocatorFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![AerodMessagePayloadType::RequestBlockLocator]),
        )),
    ];

    let invs_route = router.subscribe_with_capacity(vec![AerodMessagePayloadType::InvRelayBlock], ctx.block_invs_channel_size());
    let shared_invs_route = SharedIncomingRoute::new(invs_route);

    let num_relay_flows = (ctx.config.bps() as usize / 2).max(1);
    flows.extend((0..num_relay_flows).map(|_| {
        Box::new(HandleRelayInvsFlow::new(
            ctx.clone(),
            router.clone(),
            shared_invs_route.clone(),
            router.subscribe(vec![]),
            ibd_sender.clone(),
        )) as Box<dyn Flow>
    }));

    flows
}
