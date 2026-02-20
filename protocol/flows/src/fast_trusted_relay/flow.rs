
use crate::{
    flow_context::{BlockLogEvent, FlowContext, RequestScope},
    flow_trait::Flow,
    flowcontext::orphans::OrphanOutput,
};
use kaspa_consensus_core::{api::BlockValidationFutures, block::Block, blockstatus::BlockStatus, errors::block::RuleError};
use kaspa_consensusmanager::{BlockProcessingBatch, ConsensusProxy};
use kaspa_core::{debug, warn};
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    IncomingRoute, Router, SharedIncomingRoute,
    common::ProtocolError,
    convert::header::{HeaderFormat, Versioned},
    dequeue, dequeue_with_timeout, make_message, make_request,
    pb::{InvRelayBlockMessage, RequestBlockLocatorMessage, RequestRelayBlocksMessage, kaspad_message::Payload},
};
use kaspa_utils::channel::{JobSender, JobTrySendError as TrySendError};
use std::{collections::VecDeque, sync::Arc};
use kaspa_trusted_relay::{FastTrustedRelay, ftr_block::FtrBlock};

pub struct HandleFastTrustedRelayFlow {
    ctx: FlowContext,
    fast_trusted_relay: Arc<FastTrustedRelay>,
}

#[async_trait::async_trait]
impl Flow for HandleFastTrustedRelayFlow {
    fn router(&self) -> Option<Arc<Router>> {
        // This is considered a routerless flow, it will return none.
        None
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}


impl HandleFastTrustedRelayFlow {
    pub fn new(ctx: FlowContext, fast_trusted_relay: Arc<FastTrustedRelay>) -> Self {
        Self { ctx, fast_trusted_relay }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        debug!("{} flow started", self.name());
        loop {
            let session = self.ctx.consensus().unguarded_session();
            let is_ibd_in_transitional_state = session.async_is_consensus_in_transitional_ibd_state().await;

            let (hash, ftr_block) = match self.fast_trusted_relay.block_recv().await.recv() {
                Ok(data) => data,
                Err(e) => {
                    warn!("Fast Trusted Relay channel block receive error: {}", e);
                    continue;
                }
            };

            // We do not sync from fast relay messages, but if in transitional state,
            // toggle the fast relay off.
            if is_ibd_in_transitional_state {
                if self.fast_trusted_relay.is_relay_active() {
                    self.fast_trusted_relay.stop_fast_relay().await.unwrap();
                }
                continue;
            }

            match session.async_get_block_status(hash).await {
                None | Some(BlockStatus::StatusHeaderOnly) => {} // Continue processing this missing inv
                Some(BlockStatus::StatusInvalid) => {
                    // Report a protocol error
                    return Err(ProtocolError::OtherOwned(format!("sent inv of an invalid block {}", hash)));
                }
                _ => {
                    // Block is already known, skip to next inv
                    debug!("Relay block {} already exists, continuing...", hash);
                    continue;
                }
            }

            match self.ctx.get_orphan_roots_if_known(&session, hash).await {
                OrphanOutput::Unknown => {}           // Keep processing this inv
                OrphanOutput::NoRoots(_) => continue, // Existing orphan w/o missing roots
                OrphanOutput::Roots(_) => {
                    // This is a change to the standard relay, Since by its very nature the fast relay is only push based, we cannot enqueue,
                    // hence we just add it to the orphan pool, and let it resolve via std relay flows and logic.
                    self.ctx.add_orphan(&session, ftr_block.to_block().unwrap()).await;
                    continue;
                }
            }

            if self.ctx.is_ibd_running() && !self.ctx.should_mine(&session).await {
                // we toggle out fast relay off, since we consider it out of sync
                if self.fast_trusted_relay.is_relay_active() {
                    self.fast_trusted_relay.stop_fast_relay().await.unwrap();
                }
                debug!("Got fast relay block {} while in IBD and the node is out of sync, continuing...", hash);
                continue;
            }

            // If we were not considered synced yet we do now.
            if !self.fast_trusted_relay.is_relay_active() {
                // open the flood-gates.
                self.fast_trusted_relay.start_fast_relay().await.unwrap();
            }

            // now we start working with consensus blocks
            let block = ftr_block.to_block().unwrap();
            if block.is_header_only() {
                // TODO: check if this should be unexpected an a warn message.
                debug!("Received header-only block {} from fast relay", hash);
                continue;
            }

            let blue_work_threshold = session.async_get_virtual_merge_depth_blue_work_threshold().await;
            // Since `blue_work` respects topology, the negation of this condition means that the relay
            // block is not in the future of virtual's merge depth root, and thus cannot be merged unless
            // other valid blocks Kosherize it (in which case it will be obtained once the merger is relayed)
            let broadcast = block.header.blue_work > blue_work_threshold;

            if !broadcast {
                debug!(
                    "Fast Relay block {} has lower blue work than virtual's merge depth root ({} <= {}), hence we are skipping it",
                    hash, block.header.blue_work, blue_work_threshold
                );
                continue;
            }

            // Consider: This might be bad practice, but technically if we consider all fast relay peers in the trusted relay "trusted"
            // we could consider adding blocks with no consensus verification (only state advancements), to further speed up the synchronization of fast relay nodes.
            // at least this could reduce the latency for block templating.
            let BlockValidationFutures { block_task, mut virtual_state_task } = session.validate_and_insert_block(block.clone());

            let validated_block = match block_task.await {
                Ok(_) => block,
                Err(RuleError::MissingParents(missing_parents)) => {
                    debug!("Block {} is missing parents: {:?}", hash, missing_parents);
                    // This is a change to the standard relay, the fast relay will not handle orphans and simply add to the orphan pool and continue.
                    self.ctx.add_orphan(&session, block).await;
                    continue;
                }
                Err(rule_error) => {
                    // We don't issue protocol errors in the fast trusted relay since we consider all peers to be trusted, but we do log unexpected validation errors.
                    warn!("Fast Relay Block {} failed validation, this is unexpected: {}", hash, rule_error);
                    continue;
                },
            };

            if broadcast {
                self.ctx
                    .hub()
                    .broadcast(
                        make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(hash.into()) }),
                        None, // Because of fast relay block fragmentation, we don't consider one peer to have sent us this.
                    )
                    .await;
            }

            let ctx = self.ctx.clone();
            tokio::spawn(async move {
                ctx.on_new_block(&session, BlockProcessingBatch::default(), validated_block, virtual_state_task).await;
                ctx.log_block_event(BlockLogEvent::Relay(hash));
            });
        }
    }

    }
