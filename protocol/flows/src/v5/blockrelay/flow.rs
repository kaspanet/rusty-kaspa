use crate::{flow_context::FlowContext, flow_trait::Flow, flowcontext::orphans::ORPHAN_RESOLUTION_RANGE};
use kaspa_consensus_core::{api::DynConsensus, block::Block, blockstatus::BlockStatus, errors::block::RuleError};
use kaspa_core::{debug, info};
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue, dequeue_with_timeout, make_message,
    pb::{kaspad_message::Payload, InvRelayBlockMessage, RequestBlockLocatorMessage, RequestRelayBlocksMessage},
    IncomingRoute, Router,
};
use kaspa_utils::option::OptionExtensions;
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
            let consensus = self.ctx.consensus();

            match consensus.get_block_status(inv.hash) {
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
                self.enqueue_orphan_roots(inv.hash).await;
                continue;
            }

            if self.ctx.is_ibd_running() && !self.ctx.is_nearly_synced() {
                // Note: If the node is considered nearly synced we continue processing relay blocks even though an IBD is in progress.
                // For instance this means that downloading a side-chain from a delayed node does not interop the normal flow of live blocks.
                debug!("Got relay block {} while in IBD and the node is out of sync, continuing...", inv.hash);
                continue;
            }

            let Some(block) = self.request_block(inv.hash).await? else {
                debug!("Relay block {} was already requested from another peer, continuing...", inv.hash);
                continue;
            };

            if block.is_header_only() {
                return Err(ProtocolError::OtherOwned(format!("sent header of {} where expected block with body", block.hash())));
            }

            // Note we do not apply the heuristic below if inv was queued indirectly (as an orphan root), since
            // that means the process started by a proper and relevant relay block
            if !inv.is_indirect {
                // Check bounded merge depth to avoid requesting irrelevant data which cannot be merged under virtual
                if let Some(virtual_merge_depth_root) = consensus.get_virtual_merge_depth_root() {
                    let root_header = consensus.get_header(virtual_merge_depth_root).unwrap();
                    // Since `blue_work` respects topology, this condition means that the relay
                    // block is not in the future of virtual's merge depth root, and thus cannot be merged unless
                    // other valid blocks Kosherize it, in which case it will be obtained once the merger is relayed
                    if block.header.blue_work <= root_header.blue_work {
                        debug!(
                            "Relay block {} has lower blue work than virtual's merge depth root {} ({} <= {}), hence we are skipping it",
                            inv.hash, virtual_merge_depth_root, block.header.blue_work, root_header.blue_work
                        );
                        continue;
                    }
                }
            }

            let prev_virtual_parents = consensus.get_virtual_parents();

            // TODO: consider storing the future in a task queue and polling it (without awaiting) in order to continue
            // queueing the following relay blocks. On the other hand we might have sufficient concurrency from all parallel relay flows
            match consensus.validate_and_insert_block(block.clone(), true).await {
                Ok(_) => {}
                Err(RuleError::MissingParents(missing_parents)) => {
                    debug!("Block {} is orphan and has missing parents: {:?}", block.hash(), missing_parents);
                    self.process_orphan(&consensus, block).await?;
                    continue;
                }
                Err(rule_error) => return Err(rule_error.into()),
            }

            info!("Accepted block {} via relay", inv.hash);
            self.ctx.on_new_block(block).await?;

            // Broadcast all *new* virtual parents. As a policy, we avoid directly relaying the new block since
            // we wish to relay only blocks who entered past(virtual).
            for new_virtual_parent in consensus.get_virtual_parents().difference(&prev_virtual_parents) {
                self.ctx
                    .hub()
                    .broadcast(make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(new_virtual_parent.into()) }))
                    .await;
            }

            self.ctx.on_new_block_template().await?;
        }
    }

    async fn enqueue_orphan_roots(&mut self, orphan: Hash) {
        if let Some(roots) = self.ctx.get_orphan_roots(orphan).await {
            if roots.is_empty() {
                return;
            }
            info!("Block {} has {} missing ancestors. Adding them to the invs queue...", orphan, roots.len());
            self.invs_route.enqueue_indirect_invs(roots)
        }
    }

    async fn request_block(&mut self, requested_hash: Hash) -> Result<Option<Block>, ProtocolError> {
        // TODO: perhaps the request scope should be captured until block processing is completed
        let Some(_request_scope) = self.ctx.try_adding_block_request(requested_hash) else { return Ok(None); };
        self.router
            .enqueue(make_message!(Payload::RequestRelayBlocks, RequestRelayBlocksMessage { hashes: vec![requested_hash.into()] }))
            .await?;
        let msg = dequeue_with_timeout!(self.msg_route, Payload::Block)?;
        let block: Block = msg.try_into()?;
        if block.hash() != requested_hash {
            Err(ProtocolError::OtherOwned(format!("requested block hash {} but got block {}", requested_hash, block.hash())))
        } else {
            Ok(Some(block))
        }
    }

    async fn process_orphan(&mut self, consensus: &DynConsensus, block: Block) -> Result<(), ProtocolError> {
        // Return if the block has been orphaned from elsewhere already
        if self.ctx.is_known_orphan(block.hash()).await {
            return Ok(());
        }

        // Add the block to the orphan pool if it's within orphan resolution range
        if self.check_orphan_resolution_range(consensus, block.hash()).await? {
            let hash = block.hash();
            self.ctx.add_orphan(block).await;
            self.enqueue_orphan_roots(hash).await;
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
    /// a limit of ORPHAN_RESOLUTION_RANGE. In the response, if we know none of the hashes,
    /// we should retrieve the given blockHash via IBD. Otherwise, via unorphaning.
    async fn check_orphan_resolution_range(&mut self, consensus: &DynConsensus, hash: Hash) -> Result<bool, ProtocolError> {
        self.router
            .enqueue(make_message!(
                Payload::RequestBlockLocator,
                RequestBlockLocatorMessage { high_hash: Some(hash.into()), limit: ORPHAN_RESOLUTION_RANGE }
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.msg_route, Payload::BlockLocator)?;
        let locator_hashes: Vec<Hash> = msg.try_into()?;
        Ok(locator_hashes.into_iter().any(|p| consensus.get_block_status(p).has_value_and(|s| !s.is_header_only())))
    }
}
