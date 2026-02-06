use crate::{
    flow_context::{BlockLogEvent, FlowContext, RequestScope},
    flow_trait::Flow,
    flowcontext::orphans::OrphanOutput,
};
use kaspa_consensus_core::{api::BlockValidationFutures, block::Block, blockstatus::BlockStatus, errors::block::RuleError};
use kaspa_consensusmanager::{BlockProcessingBatch, ConsensusProxy};
use kaspa_core::debug;
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    IncomingRoute, Router, SharedIncomingRoute,
    common::ProtocolError,
    convert::header::{HeaderFormat, Versioned},
    dequeue_with_timeout, dequeue_with_timestamp, make_message, make_request,
    pb::{InvRelayBlockMessage, RequestBlockLocatorMessage, RequestRelayBlocksMessage, kaspad_message::Payload},
};
use kaspa_utils::channel::{JobSender, JobTrySendError as TrySendError};
use std::{collections::VecDeque, sync::Arc, time::Instant};

pub struct RelayInvMessage {
    hash: Hash,

    /// Indicates whether this inv is an orphan root of a previously relayed descendent
    /// (i.e. this inv was indirectly queued)
    is_orphan_root: bool,

    /// Indicates whether this inv is already known to be within orphan resolution range
    known_within_range: bool,

    // Time when this message was first dequeued -> of interest only for direct invs in conjunction with pergiee
    timestamp: Option<Instant>,
}

/// Encapsulates an incoming invs route which also receives data locally
pub struct TwoWayIncomingRoute {
    incoming_route: SharedIncomingRoute,
    indirect_invs: VecDeque<RelayInvMessage>,
}

impl TwoWayIncomingRoute {
    pub fn new(incoming_route: SharedIncomingRoute) -> Self {
        Self { incoming_route, indirect_invs: VecDeque::new() }
    }

    pub fn enqueue_indirect_invs<I: IntoIterator<Item = Hash>>(&mut self, iter: I, known_within_range: bool) {
        // All indirect invs are orphan roots; not all are known to be within orphan resolution range
        self.indirect_invs.extend(iter.into_iter().map(|h| RelayInvMessage {
            hash: h,
            is_orphan_root: true,
            known_within_range,
            timestamp: None,
        }))
    }

    pub async fn dequeue(&mut self) -> Result<RelayInvMessage, ProtocolError> {
        if let Some(inv) = self.indirect_invs.pop_front() {
            Ok(inv)
        } else {
            let (msg, ts) = dequeue_with_timestamp!(self.incoming_route, Payload::InvRelayBlock)?;
            let inv = msg.try_into()?;
            Ok(RelayInvMessage { hash: inv, is_orphan_root: false, known_within_range: false, timestamp: Some(ts) })
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
    ibd_sender: JobSender<Block>,
    /// Header format determined by protocol version
    header_format: HeaderFormat,
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
        invs_route: SharedIncomingRoute,
        msg_route: IncomingRoute,
        ibd_sender: JobSender<Block>,
        header_format: HeaderFormat,
    ) -> Self {
        Self { ctx, router, invs_route: TwoWayIncomingRoute::new(invs_route), msg_route, ibd_sender, header_format }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            // Loop over incoming block inv messages
            let inv = self.invs_route.dequeue().await?;
            let session = self.ctx.consensus().unguarded_session();
            let is_ibd_in_transitional_state = session.async_is_consensus_in_transitional_ibd_state().await;

            match session.async_get_block_status(inv.hash).await {
                None | Some(BlockStatus::StatusHeaderOnly) => {} // Continue processing this missing inv
                Some(BlockStatus::StatusInvalid) => {
                    // Report a protocol error
                    return Err(ProtocolError::OtherOwned(format!("sent inv of an invalid block {}", inv.hash)));
                }
                _ => {
                    debug!("Relay block {} already exists, continuing...", inv.hash);
                    if should_signal_perigee(&self.ctx, &inv, self.ctx.is_ibd_running()) {
                        self.spawn_perigee_timestamp_signal(inv.hash, inv.timestamp.unwrap(), false);
                    }
                    continue;
                }
            }

            match self.ctx.get_orphan_roots_if_known(&session, inv.hash).await {
                OrphanOutput::Unknown => {} // Keep processing this inv
                OrphanOutput::NoRoots(_) => {
                    if should_signal_perigee(&self.ctx, &inv, self.ctx.is_ibd_running()) {
                        self.spawn_perigee_timestamp_signal(inv.hash, inv.timestamp.unwrap(), false);
                    }
                } // Existing orphan w/o missing roots
                OrphanOutput::Roots(roots) => {
                    // Known orphan with roots to enqueue
                    self.enqueue_orphan_roots(inv.hash, roots, inv.known_within_range);
                    if should_signal_perigee(&self.ctx, &inv, self.ctx.is_ibd_running()) {
                        self.spawn_perigee_timestamp_signal(inv.hash, inv.timestamp.unwrap(), false);
                    }
                    continue;
                }
            }

            if self.ctx.is_ibd_running() && !self.ctx.should_mine(&session).await {
                // Note: If the node is considered nearly synced we continue processing relay blocks even though an IBD is in progress.
                // For instance this means that downloading a side-chain from a delayed node does not interop the normal flow of live blocks.
                debug!("Got relay block {} while in IBD and the node is out of sync, continuing...", inv.hash);
                continue;
            }

            // We keep the request scope alive until consensus processes the block
            let Some((block, request_scope)) = self.request_block(inv.hash, self.msg_route.id(), self.header_format).await? else {
                debug!("Relay block {} was already requested from another peer, continuing...", inv.hash);
                if should_signal_perigee(&self.ctx, &inv, self.ctx.is_ibd_running()) {
                    self.spawn_perigee_timestamp_signal(inv.hash, inv.timestamp.unwrap(), false);
                }
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
            if !inv.is_orphan_root && !broadcast {
                debug!(
                    "Relay block {} has lower blue work than virtual's merge depth root ({} <= {}), hence we are skipping it",
                    inv.hash, block.header.blue_work, blue_work_threshold
                );
                continue;
            }
            // if in a transitional ibd state, do not wait, sync immediately
            if is_ibd_in_transitional_state {
                self.try_trigger_ibd(block)?;
                continue;
            }

            let BlockValidationFutures { block_task, mut virtual_state_task } = session.validate_and_insert_block(block.clone());

            let ancestor_batch = match block_task.await {
                Ok(_) => Default::default(),
                Err(RuleError::MissingParents(missing_parents)) => {
                    debug!("Block {} is orphan and has missing parents: {:?}", block.hash(), missing_parents);
                    if let Some(mut ancestor_batch) = self.process_orphan(&session, block.clone(), inv.known_within_range).await? {
                        // Block is not an orphan, retrying
                        let BlockValidationFutures { block_task: block_task_inner, virtual_state_task: virtual_state_task_inner } =
                            session.validate_and_insert_block(block.clone());
                        virtual_state_task = virtual_state_task_inner;
                        for block_task in ancestor_batch.block_tasks.take().unwrap() {
                            match block_task.await {
                                Ok(_) => {}
                                // We disconnect on invalidness even though this is not a direct relay from this peer, because
                                // current relay is a descendant of this block (i.e. this peer claims all its ancestors are valid)
                                Err(rule_error) => return Err(rule_error.into()),
                            }
                        }

                        match block_task_inner.await {
                            Ok(_) => match ancestor_batch.blocks.len() {
                                0 => debug!("Retried orphan block {} successfully", block.hash()),
                                n => {
                                    self.ctx.log_block_event(BlockLogEvent::Unorphaned(ancestor_batch.blocks[0].hash(), n));
                                    debug!("Unorphaned {} ancestors and retried orphan block {} successfully", n, block.hash())
                                }
                            },
                            Err(rule_error) => return Err(rule_error.into()),
                        }
                        ancestor_batch
                    } else {
                        if should_signal_perigee(&self.ctx, &inv, self.ctx.is_ibd_running()) {
                            self.spawn_perigee_timestamp_signal(inv.hash, inv.timestamp.unwrap(), false);
                        }
                        continue;
                    }
                }
                Err(rule_error) => return Err(rule_error.into()),
            };

            // As a policy, we only relay blocks who stand a chance to enter past(virtual).
            // The only mining rule which permanently excludes a block is the merge depth bound
            // (as opposed to "max parents" and "mergeset size limit" rules)
            if broadcast {
                let msgs = ancestor_batch
                    .blocks
                    .iter()
                    .map(|b| make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(b.hash().into()) }))
                    .collect();
                // we filter out the current peer to avoid sending it back invs we know it already has
                self.ctx.hub().broadcast_many(msgs, Some(self.router.key())).await;

                // we filter out the current peer to avoid sending it back the same invs
                self.ctx
                    .hub()
                    .broadcast(
                        make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(inv.hash.into()) }),
                        Some(self.router.key()),
                    )
                    .await;
            }

            // We spawn post-processing as a separate task so that this loop
            // can continue processing the following relay blocks
            let ctx = self.ctx.clone();
            let router = self.router.clone();
            tokio::spawn(async move {
                ctx.on_new_block(&session, ancestor_batch, block, virtual_state_task).await;
                if should_signal_perigee(&ctx, &inv, ctx.is_ibd_running()) {
                    ctx.maybe_add_perigee_timestamp(router, inv.hash, inv.timestamp.unwrap(), true).await;
                }
                ctx.log_block_event(BlockLogEvent::Relay(inv.hash));
            });
        }
    }

    fn spawn_perigee_timestamp_signal(&self, hash: Hash, timestamp: Instant, verify: bool) {
        let ctx = self.ctx.clone();
        let router = self.router.clone();

        tokio::spawn(async move {
            ctx.maybe_add_perigee_timestamp(router, hash, timestamp, verify).await;
        });
    }

    fn enqueue_orphan_roots(&mut self, _orphan: Hash, roots: Vec<Hash>, known_within_range: bool) {
        self.invs_route.enqueue_indirect_invs(roots, known_within_range)
    }

    async fn request_block(
        &mut self,
        requested_hash: Hash,
        request_id: u32,
        header_format: HeaderFormat,
    ) -> Result<Option<(Block, RequestScope<Hash>)>, ProtocolError> {
        // Note: the request scope is returned and should be captured until block processing is completed
        let Some(request_scope) = self.ctx.try_adding_block_request(requested_hash) else {
            return Ok(None);
        };
        self.router
            .enqueue(make_request!(
                Payload::RequestRelayBlocks,
                RequestRelayBlocksMessage { hashes: vec![requested_hash.into()] },
                request_id
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.msg_route, Payload::Block)?;
        let block: Block = Versioned(header_format, msg).try_into()?;
        if block.hash() != requested_hash {
            Err(ProtocolError::OtherOwned(format!("requested block hash {} but got block {}", requested_hash, block.hash())))
        } else {
            Ok(Some((block, request_scope)))
        }
    }

    /// Process the orphan block. Returns `Some(BlockProcessingBatch)` if the block has no missing roots, where
    /// the batch includes ancestor blocks and their consensus processing batch. This indicates a retry is recommended.
    async fn process_orphan(
        &mut self,
        consensus: &ConsensusProxy,
        block: Block,
        mut known_within_range: bool,
    ) -> Result<Option<BlockProcessingBatch>, ProtocolError> {
        // Return if the block has been orphaned from elsewhere already
        if self.ctx.is_known_orphan(block.hash()).await {
            return Ok(None);
        }

        /* We orphan a block if one of the following holds:
                1. It is known to be within orphan resolution range (no-op)
                2. It holds the IBD DAA score heuristic conditions (local op)
                3. We resolve its orphan range by interacting with the peer (peer op)

            Note that we check the conditions by the order of their cost and avoid making expensive calls if not needed.
        */
        let should_orphan = known_within_range || self.check_orphan_ibd_conditions(block.header.daa_score) || {
            // Inner scope to evaluate orphan resolution range and reassign the `known_within_range` variable
            known_within_range = self.check_orphan_resolution_range(consensus, block.hash(), self.msg_route.id()).await?;
            known_within_range
        };

        if should_orphan {
            let hash = block.hash();
            match self.ctx.add_orphan(consensus, block).await {
                // There is a sync gap between consensus and the orphan pool, meaning that consensus might have indicated
                // that this block is orphan, but by the time it got to the orphan pool we discovered it no longer has missing roots.
                // In such a case, the orphan pool will queue the known orphan ancestors to consensus and will return the block processing
                // batch.
                // We signal this to the caller by returning the batch of processed ancestors, indicating a consensus processing retry
                // should be performed for this block as well.
                Some(OrphanOutput::NoRoots(ancestor_batch)) => {
                    return Ok(Some(ancestor_batch));
                }
                Some(OrphanOutput::Roots(roots)) => {
                    self.ctx.log_block_event(BlockLogEvent::Orphaned(hash, roots.len()));
                    self.enqueue_orphan_roots(hash, roots, known_within_range)
                }
                None | Some(OrphanOutput::Unknown) => {}
            }
        } else {
            self.try_trigger_ibd(block)?;
        }
        Ok(None)
    }

    /// Applies an heuristic to check whether we should store the orphan block in the orphan pool for IBD considerations.
    ///
    /// When IBD is going on it is guaranteed to sync all blocks in past(R) where R is the relay block triggering the
    /// IBD. Frequently, if the IBD is short and fast enough, R will be within short distance from the syncer tips once
    /// the IBD is over. However antipast(R) is usually not in orphan resolution range so these blocks will not be kept
    /// leading to another IBD and so on.
    ///
    /// By checking whether the current orphan DAA score is within the range (R - M/10, R + M/2)** we make sure that in this
    /// case we keep ~M/2 blocks in the orphan pool which are all unorphaned when IBD completes (see revalidate_orphans),
    /// and the node reaches full sync state asap. We use M/10 for the lower bound since we only want to cover anticone(R)
    /// in that region (which is expectedly small), whereas the M/2 upper bound is for covering the most early segment in
    /// future(R). Overall we avoid keeping more than ~M/2 in order to not enter the area where blocks start getting evicted
    /// from the orphan pool.
    ///
    /// **where R is the DAA score of R, and M is the orphans pool size limit
    fn check_orphan_ibd_conditions(&self, orphan_daa_score: u64) -> bool {
        if let Some(ibd_daa_score) = self.ctx.ibd_relay_daa_score() {
            let max_orphans = self.ctx.max_orphans() as u64;
            orphan_daa_score + max_orphans / 10 > ibd_daa_score && orphan_daa_score < ibd_daa_score + max_orphans / 2
        } else {
            false
        }
    }

    /// Checks whether the given block hash is within orphan resolution range. This method sends a BlockLocator
    /// request to the peer with a limit of `ctx.orphan_resolution_range`. In the response, if we know one of the
    /// hashes, we should retrieve the given block via unorphaning.
    async fn check_orphan_resolution_range(
        &mut self,
        consensus: &ConsensusProxy,
        hash: Hash,
        request_id: u32,
    ) -> Result<bool, ProtocolError> {
        self.router
            .enqueue(make_request!(
                Payload::RequestBlockLocator,
                RequestBlockLocatorMessage { high_hash: Some(hash.into()), limit: self.ctx.orphan_resolution_range() },
                request_id
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.msg_route, Payload::BlockLocator)?;
        let locator_hashes: Vec<Hash> = msg.try_into()?;
        // Locator hashes are sent from later to earlier, so it makes sense to query consensus in reverse. Technically
        // with current syncer-side implementations (in both go-kaspa and this codebase) we could query only the last one,
        // but we prefer not relying on such details for correctness
        //
        // The current syncer-side implementation sends a full locator even though it suffices to only send the
        // most early block. We keep it this way in order to allow future syncee-side implementations to do more
        // with the full incremental info and because it is only a small set of hashes.
        for h in locator_hashes.into_iter().rev() {
            if consensus.async_get_block_status(h).await.is_some_and(|s| s.has_block_body()) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    // Send the block to IBD flow via the dedicated job channel. If the channel has a pending job, we prefer
    // the block with higher blue work, since it is usually more recent
    fn try_trigger_ibd(&self, block: Block) -> Result<(), ProtocolError> {
        match self.ibd_sender.try_send(block.clone(), |b, c| if b.header.blue_work > c.header.blue_work { b } else { c }) {
            Ok(_) | Err(TrySendError::Full(_)) => Ok(()),
            Err(TrySendError::Closed(_)) => Err(ProtocolError::ConnectionClosed), // This indicates that IBD flow has exited
        }
    }
}

fn should_signal_perigee(ctx: &FlowContext, inv: &RelayInvMessage, is_ibd_running: bool) -> bool {
    !inv.is_orphan_root && ctx.is_perigee_active() && !is_ibd_running
}
