use crate::{flow_context::FlowContext, flow_trait::Flow};
use consensus_core::{block::Block, blockstatus::BlockStatus};
use hashes::Hash;
use p2p_lib::{
    common::ProtocolError,
    dequeue, dequeue_with_timeout, make_message,
    pb::{kaspad_message::Payload, RequestRelayBlocksMessage},
    IncomingRoute, Router,
};
use std::{collections::VecDeque, sync::Arc};

pub struct RelayInvMessage {
    hash: Hash,
    is_indirect: bool,
}

/// Encapsulates an incoming invs route which also receives data locally
pub struct TwoWayIncomingRoute {
    incoming_route: IncomingRoute,
    indirect_invs: VecDeque<Hash>,
}

impl TwoWayIncomingRoute {
    pub fn new(incoming_route: IncomingRoute) -> Self {
        Self { incoming_route, indirect_invs: VecDeque::new() }
    }

    pub fn enqueue_indirect_invs<I: IntoIterator<Item = Hash>>(&mut self, iter: I) {
        self.indirect_invs.extend(iter)
    }

    pub async fn dequeue(&mut self) -> Result<RelayInvMessage, ProtocolError> {
        if let Some(inv) = self.indirect_invs.pop_front() {
            Ok(RelayInvMessage { hash: inv, is_indirect: true })
        } else {
            let msg = dequeue!(self.incoming_route, Payload::InvRelayBlock)?;
            let inv = msg.try_into()?;
            Ok(RelayInvMessage { hash: inv, is_indirect: false })
        }
    }
}

pub struct HandleRelayInvsFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    /// A route specific for invs messages
    invs_route: TwoWayIncomingRoute,
    /// A route for other messages such as Block and BlockLocator
    msg_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for HandleRelayInvsFlow {
    fn name(&self) -> &'static str {
        "HANDLE_RELAY_INVS"
    }

    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl HandleRelayInvsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, invs_route: IncomingRoute, msg_route: IncomingRoute) -> Self {
        Self { ctx, router, invs_route: TwoWayIncomingRoute::new(invs_route), msg_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let inv = self.invs_route.dequeue().await?;
            let consensus = self.ctx.consensus();
            match consensus.get_block_status(inv.hash) {
                None | Some(BlockStatus::StatusHeaderOnly) => {} // Continue processing this missing inv
                Some(BlockStatus::StatusInvalid) => {
                    // Report a protocol error
                    return Err(ProtocolError::OtherOwned(format!("sent inv of an invalid block {}", inv.hash)));
                }
                _ => continue, // Block is already known, skip to next inv
            }

            if self.ctx.is_orphan(inv.hash).await {
                // TODO: check for config conditions
                self.enqueue_orphan_roots(inv.hash).await;
                continue;
            }

            // TODO: check if IBD is running and node is not nearly synced

            let block = self.request_block(inv.hash).await?;

            if block.is_header_only() {
                return Err(ProtocolError::OtherOwned(format!(
                    "sent block header of {} where expected block with body",
                    block.hash()
                )));
            }

            // TODO: check for config conditions

            // Note we do not apply the heuristic below if inv was queued indirectly (as an orphan root), since
            // that means the process started by a proper and relevant relay block
            if !inv.is_indirect {
                // TODO: imp merge depth root heuristic
            }

            // TODO: process the block
        }
    }

    async fn enqueue_orphan_roots(&mut self, orphan: Hash) {
        if let Some(roots) = self.ctx.get_orphan_roots(orphan).await {
            self.invs_route.enqueue_indirect_invs(roots)
        } else {
            // TODO: log
        }
    }

    async fn request_block(&mut self, request_hash: Hash) -> Result<Block, ProtocolError> {
        // TODO: manage shared requests and return `exists` if it's already a pending request
        self.router
            .enqueue(make_message!(Payload::RequestRelayBlocks, RequestRelayBlocksMessage { hashes: vec![request_hash.into()] }))
            .await?;
        let msg = dequeue_with_timeout!(self.msg_route, Payload::Block)?;
        let block: Block = msg.try_into()?;
        if block.hash() != request_hash {
            Err(ProtocolError::OtherOwned(format!("requested block hash {} but got block {}", request_hash, block.hash())))
        } else {
            Ok(block)
        }
    }
}
