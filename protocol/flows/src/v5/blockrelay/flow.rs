use crate::{
    flow_context::{BlockSource, FlowContext, RequestScope},
    flow_trait::Flow,
};
use kaspa_consensus_core::{api::BlockValidationFutures, block::Block, blockstatus::BlockStatus, errors::block::RuleError};
use kaspa_consensusmanager::ConsensusProxy;
use kaspa_core::{debug, info};
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue, dequeue_with_timeout, make_message,
    pb::{kaspad_message::Payload, InvRelayBlockMessage, RequestBlockLocatorMessage, RequestRelayBlocksMessage},
    IncomingRoute, Router,
};
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::mpsc::{error::TrySendError, Sender};

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
    /// A channel sender for sending blocks to be handled by the IBD flow (of this peer)
    ibd_sender: Sender<Block>,
}

#[async_trait::async_trait]
impl Flow for HandleRelayInvsFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl HandleRelayInvsFlow {
    pub fn new(
        ctx: FlowContext,
        router: Arc<Router>,
        invs_route: IncomingRoute,
        msg_route: IncomingRoute,
        ibd_sender: Sender<Block>,
    ) -> Self {
        Self { ctx, router, invs_route: TwoWayIncomingRoute::new(invs_route), msg_route, ibd_sender }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            // Loop over incoming block inv messages
            let inv = self.invs_route.dequeue().await?;
            let session = self.ctx.consensus().unguarded_session();

            match session.async_get_block_status(inv.hash).await {
                None | Some(BlockStatus::StatusHeaderOnly) => {} // Continue processing this missing inv
                Some(BlockStatus::StatusInvalid) => {
                    // Report a protocol error
                    return Err(ProtocolError::OtherOwned(format!("sent inv of an invalid block {}", inv.hash)));
                }
                _ => {
                    // Block is already known, skip to next inv
                    debug!("Relay block {} already exists, continuing...", inv.hash);
                    continue;
                }
            }

            if self.ctx.is_known_orphan(inv.hash).await {
                self.enqueue_orphan_roots(&session, inv.hash).await;
                continue;
            }

            if self.ctx.is_ibd_running() && !session.async_is_nearly_synced().await {
                // Note: If the node is considered nearly synced we continue processing relay blocks even though an IBD is in progress.
                // For instance this means that downloading a side-chain from a delayed node does not interop the normal flow of live blocks.
                debug!("Got relay block {} while in IBD and the node is out of sync, continuing...", inv.hash);
                continue;
            }

            // We keep the request scope alive until consensus processes the block
            let Some((block, request_scope)) = self.request_block(inv.hash).await? else {
                debug!("Relay block {} was already requested from another peer, continuing...", inv.hash);
                continue;
            };
            request_scope.report_obtained();

            if block.is_header_only() {
                return Err(ProtocolError::OtherOwned(format!("sent header of {} where expected block with body", block.hash())));
            }

            let blue_work_threshold = session.async_get_virtual_merge_depth_blue_work_threshold().await;
            // Since `blue_work` respects topology, the negation of this condition means that the relay
            // block is not in the future of virtual's merge depth root, and thus cannot be merged unless
            // other valid blocks Kosherize it (in which case it will be obtained once the merger is relayed)
            let broadcast = block.header.blue_work > blue_work_threshold;

            // We do not apply the skip heuristic below if inv was queued indirectly (as an orphan root), since
            // that means the process started by a proper and relevant relay block
            if !inv.is_indirect && !broadcast {
                debug!(
                    "Relay block {} has lower blue work than virtual's merge depth root ({} <= {}), hence we are skipping it",
                    inv.hash, block.header.blue_work, blue_work_threshold
                );
                continue;
            }

            let BlockValidationFutures { block_task, virtual_state_task } = session.validate_and_insert_block(block.clone());

            match block_task.await {
                Ok(_) => {}
                Err(RuleError::MissingParents(missing_parents)) => {
                    debug!("Block {} is orphan and has missing parents: {:?}", block.hash(), missing_parents);
                    self.process_orphan(&session, block, inv.is_indirect).await?;
                    continue;
                }
                Err(rule_error) => return Err(rule_error.into()),
            }

            // As a policy, we only relay blocks who stand a chance to enter past(virtual).
            // The only mining rule which permanently excludes a block is the merge depth bound
            // (as opposed to "max parents" and "mergeset size limit" rules)
            if broadcast {
                self.ctx
                    .hub()
                    .broadcast(make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(inv.hash.into()) }))
                    .await;
            }

            // We spawn post-processing as a separate task so that this loop
            // can continue processing the following relay blocks
            let ctx = self.ctx.clone();
            tokio::spawn(async move {
                ctx.on_new_block(&session, block, virtual_state_task).await;
                ctx.on_new_block_template().await;
                ctx.log_block_acceptance(inv.hash, BlockSource::Relay);
            });
        }
    }

    async fn enqueue_orphan_roots(&mut self, consensus: &ConsensusProxy, orphan: Hash) {
        if let Some(roots) = self.ctx.get_orphan_roots(consensus, orphan).await {
            if roots.is_empty() {
                return;
            }
            if self.ctx.is_log_throttled() {
                debug!("Block {} has {} missing ancestors. Adding them to the invs queue...", orphan, roots.len());
            } else {
                info!("Block {} has {} missing ancestors. Adding them to the invs queue...", orphan, roots.len());
            }
            self.invs_route.enqueue_indirect_invs(roots)
        }
    }

    async fn request_block(&mut self, requested_hash: Hash) -> Result<Option<(Block, RequestScope<Hash>)>, ProtocolError> {
        // Note: the request scope is returned and should be captured until block processing is completed
        let Some(request_scope) = self.ctx.try_adding_block_request(requested_hash) else {
            return Ok(None);
        };
        self.router
            .enqueue(make_message!(Payload::RequestRelayBlocks, RequestRelayBlocksMessage { hashes: vec![requested_hash.into()] }))
            .await?;
        let msg = dequeue_with_timeout!(self.msg_route, Payload::Block)?;
        let block: Block = msg.try_into()?;
        if block.hash() != requested_hash {
            Err(ProtocolError::OtherOwned(format!("requested block hash {} but got block {}", requested_hash, block.hash())))
        } else {
            Ok(Some((block, request_scope)))
        }
    }

    async fn process_orphan(&mut self, consensus: &ConsensusProxy, block: Block, is_indirect_inv: bool) -> Result<(), ProtocolError> {
        // Return if the block has been orphaned from elsewhere already
        if self.ctx.is_known_orphan(block.hash()).await {
            return Ok(());
        }

        // Add the block to the orphan pool if it's within orphan resolution range.
        // If the block is indirect it means one of its descendants was already is resolution range, so
        // we can avoid the query.
        if is_indirect_inv || self.check_orphan_resolution_range(consensus, block.hash()).await? {
            let hash = block.hash();
            self.ctx.add_orphan(block).await;
            self.enqueue_orphan_roots(consensus, hash).await;
        } else {
            // Send the block to IBD flow via the dedicated channel.
            // Note that this is a non-blocking send and we don't care about being rejected if channel is full,
            // since if IBD is already running, there is no need to trigger it
            match self.ibd_sender.try_send(block) {
                Ok(_) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Closed(_)) => return Err(ProtocolError::ConnectionClosed), // This indicates that IBD flow has exited
            }
        }
        Ok(())
    }

    /// Finds out whether the given block hash should be retrieved via the unorphaning
    /// mechanism or via IBD. This method sends a BlockLocator request to the peer with
    /// a limit of `ctx.orphan_resolution_range`. In the response, if we know none of the hashes,
    /// we should retrieve the given block `hash` via IBD. Otherwise, via unorphaning.
    async fn check_orphan_resolution_range(&mut self, consensus: &ConsensusProxy, hash: Hash) -> Result<bool, ProtocolError> {
        self.router
            .enqueue(make_message!(
                Payload::RequestBlockLocator,
                RequestBlockLocatorMessage { high_hash: Some(hash.into()), limit: self.ctx.orphan_resolution_range() }
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.msg_route, Payload::BlockLocator)?;
        let locator_hashes: Vec<Hash> = msg.try_into()?;
        for h in locator_hashes {
            if consensus.async_get_block_status(h).await.is_some_and(|s| s.has_block_body()) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
