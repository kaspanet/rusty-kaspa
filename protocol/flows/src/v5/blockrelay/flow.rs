use crate::{flow_context::FlowContext, flow_trait::Flow, flowcontext::orphans::ORPHAN_RESOLUTION_RANGE};
use consensus_core::{block::Block, blockstatus::BlockStatus, errors::block::RuleError};
use hashes::Hash;
use kaspa_core::{debug, info, time::unix_now};
use kaspa_utils::option::OptionExtensions;
use p2p_lib::{
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
                _ => continue, // Block is already known, skip to next inv
            }

            if self.ctx.is_known_orphan(inv.hash).await {
                // TODO: check for config conditions
                self.enqueue_orphan_roots(inv.hash).await;
                continue;
            }

            // TODO: check if IBD is running and node is not nearly synced
            if self.ctx.is_ibd_running() {
                // TODO: fix consensus call to avoid Option
                let sink_timestamp = consensus.get_sink_timestamp();
                // TODO: use config
                if sink_timestamp.is_none() || unix_now() > sink_timestamp.unwrap() + 2641 * 1000 {
                    continue;
                }
            }

            let block = self.request_block(inv.hash).await?;

            if block.is_header_only() {
                return Err(ProtocolError::OtherOwned(format!("sent header of {} where expected block with body", block.hash())));
            }

            // TODO: check for config conditions

            // Note we do not apply the heuristic below if inv was queued indirectly (as an orphan root), since
            // that means the process started by a proper and relevant relay block
            if !inv.is_indirect {
                // TODO: imp merge depth root heuristic
            }

            match consensus.validate_and_insert_block(block.clone(), true).await {
                Ok(_) => {}
                Err(RuleError::MissingParents(missing_parents)) => {
                    debug!("Block {} is orphan and has missing parents: {:?}", block.hash(), missing_parents);
                    self.process_orphan(block).await?;
                    continue;
                }
                Err(rule_error) => return Err(rule_error.into()),
            }

            info!("Accepted block {} via relay", inv.hash);

            // TODO: FIX
            let _blocks = self.ctx.unorphan_blocks(block.hash()).await;

            // TODO: broadcast all new blocks in past(virtual)
            // TEMP:
            self.router
                .broadcast(make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(block.hash().into()) }))
                .await;
        }
    }

    async fn enqueue_orphan_roots(&mut self, orphan: Hash) {
        if let Some(roots) = self.ctx.get_orphan_roots(orphan).await {
            self.invs_route.enqueue_indirect_invs(roots)
        } else {
            // TODO: log
        }
    }

    async fn request_block(&mut self, requested_hash: Hash) -> Result<Block, ProtocolError> {
        // TODO: manage shared requests and return `exists` if it's already a pending request
        self.router
            .enqueue(make_message!(Payload::RequestRelayBlocks, RequestRelayBlocksMessage { hashes: vec![requested_hash.into()] }))
            .await?;
        let msg = dequeue_with_timeout!(self.msg_route, Payload::Block)?;
        let block: Block = msg.try_into()?;
        if block.hash() != requested_hash {
            Err(ProtocolError::OtherOwned(format!("requested block hash {} but got block {}", requested_hash, block.hash())))
        } else {
            Ok(block)
        }
    }

    async fn process_orphan(&mut self, block: Block) -> Result<(), ProtocolError> {
        // Return if the block has been orphaned from elsewhere already
        if self.ctx.is_known_orphan(block.hash()).await {
            return Ok(());
        }

        // Add the block to the orphan pool if it's within orphan resolution range
        if self.check_orphan_resolution_range(block.hash()).await? {
            // TODO: check for config conditions
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

    /// Finds out whether the given blockHash should be retrieved via the unorphaning
    /// mechanism or via IBD. This method sends a BlockLocator request to the peer with
    /// a limit of ORPHAN_RESOLUTION_RANGE. In the response, if we know none of the hashes,
    /// we should retrieve the given blockHash via IBD. Otherwise, via unorphaning.
    async fn check_orphan_resolution_range(&mut self, hash: Hash) -> Result<bool, ProtocolError> {
        self.router
            .enqueue(make_message!(
                Payload::RequestBlockLocator,
                RequestBlockLocatorMessage { high_hash: Some(hash.into()), limit: ORPHAN_RESOLUTION_RANGE }
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.msg_route, Payload::BlockLocator)?;
        let locator_hashes: Vec<Hash> = msg.try_into()?;
        let consensus = self.ctx.consensus(); // TODO: should we pass the consensus instance through the call chain?
        Ok(locator_hashes.into_iter().any(|p| consensus.get_block_status(p).has_value_and(|s| !s.is_header_only())))
    }
}
