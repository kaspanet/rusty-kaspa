use crate::{
    flow_context::{BlockLogEvent, FlowContext, RequestScope},
    flow_trait::Flow,
    flowcontext::orphans::OrphanOutput,
};
use kaspa_consensus_core::{api::BlockValidationFutures, block::Block, blockstatus::BlockStatus, errors::block::RuleError};
use kaspa_consensusmanager::{BlockProcessingBatch, ConsensusProxy};
use kaspa_core::{debug, info, warn};
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    IncomingRoute, Router, SharedIncomingRoute,
    common::ProtocolError,
    convert::header::{HeaderFormat, Versioned},
    dequeue, dequeue_with_timeout, make_message, make_request,
    pb::{InvRelayBlockMessage, RequestBlockLocatorMessage, RequestRelayBlocksMessage, kaspad_message::Payload},
};
use kaspa_trusted_relay::{FastTrustedRelay, fast_trusted_relay, model::ftr_block::FtrBlock};
use kaspa_utils::channel::{JobSender, JobTrySendError as TrySendError};
use kaspa_utils::triggers::Listener;
use std::{collections::VecDeque, sync::Arc};

// TODO: implement more intricate orphan handling.

pub struct HandleFastTrustedRelayFlow {
    ctx: FlowContext,
    fast_trusted_relay: FastTrustedRelay,
    shutdown_listener: Listener,
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
    pub fn new(ctx: FlowContext, fast_trusted_relay: FastTrustedRelay, shutdown_listener: Listener) -> Self {
        Self { ctx, fast_trusted_relay, shutdown_listener }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        info!("{} flow started", self.name());
        // TCP control runtime is auto-spawned on FastTrustedRelay::new()
        loop {
            let session = self.ctx.consensus().unguarded_session();
            let is_ibd_in_transitional_state = session.async_is_consensus_in_transitional_ibd_state().await;

            debug!("Waiting to receive block from fast trusted relay...");

            // Use select! to handle graceful shutdown
            let (hash, ftr_block) = tokio::select! {
                biased;
                _ = self.shutdown_listener.clone() => {
                    info!("{} flow received shutdown signal, exiting gracefully", self.name());
                    return Ok(());
                }
                result = self.fast_trusted_relay.recv_block() => result,
            };

            debug!("Received block {} from fast trusted relay", hash);

            // We do not sync from fast relay messages, but if in transitional state,
            // toggle the fast relay off.
            if is_ibd_in_transitional_state {
                if self.fast_trusted_relay.is_udp_active().await {
                    self.fast_trusted_relay.stop_fast_relay().await;
                }
                continue;
            }

            match session.async_get_block_status(hash).await {
                None | Some(BlockStatus::StatusHeaderOnly) => {} // Continue processing this missing inv
                Some(BlockStatus::StatusInvalid) => {
                    // Report a protocol error
                    warn!("Fast Trusted Relay sent inv of an invalid block {}", hash);
                }
                _ => {
                    // Block is already known, skip to next inv
                    info!("Relay block {} already exists in consensus, skipping", hash);
                    continue;
                }
            }

            match self.ctx.get_orphan_roots_if_known(&session, hash).await {
                OrphanOutput::Unknown => {} // Keep processing this inv
                OrphanOutput::NoRoots(_) => {
                    info!("Block {} is already in orphan pool with no missing roots, skipping", hash);
                    continue;
                }
                OrphanOutput::Roots(roots) => {
                    // This is a change to the standard relay, Since by its very nature the fast relay is only push based, we cannot enqueue,
                    // hence we just add it to the orphan pool, and let it resolve via std relay flows and logic.
                    info!("Block {} has {} missing parent roots, adding to orphan pool", hash, roots.len());
                    self.ctx.add_orphan(&session, ftr_block.into()).await;
                    continue;
                }
            }

            if self.ctx.is_ibd_running() && !self.ctx.should_mine(&session).await {
                // we toggle out fast relay off, since we consider it out of sync
                if self.fast_trusted_relay.is_udp_active().await {
                    self.fast_trusted_relay.stop_fast_relay().await;
                }
                info!("Got fast relay block {} while in IBD and the node is out of sync, skipping (relay disabled)", hash);
                continue;
            }

            // If we were not considered synced yet we do now.
            if !self.fast_trusted_relay.is_udp_active().await {
                info!("Turning on fast trusted relay UDP transport since we consider ourselves synced now");
                self.fast_trusted_relay.start_fast_relay().await;
            }

            // now we start working with consensus blocks
            let block = Block::from(ftr_block);
            if block.is_header_only() {
                // TODO: check if this should be unexpected an a warn message.
                info!("Received header-only block {} from fast relay, skipping", hash);
                continue;
            }

            let blue_work_threshold = session.async_get_virtual_merge_depth_blue_work_threshold().await;
            // Since `blue_work` respects topology, the negation of this condition means that the relay
            // block is not in the future of virtual's merge depth root, and thus cannot be merged unless
            // other valid blocks Kosherize it (in which case it will be obtained once the merger is relayed)
            let broadcast = block.header.blue_work > blue_work_threshold;

            if !broadcast {
                warn!(
                    "Fast Relay block {} has lower blue work than virtual's merge depth root ({} <= {}), hence we are skipping it",
                    hash, block.header.blue_work, blue_work_threshold
                );
                continue;
            }

            // Consider: This might be bad practice, but technically if we consider all fast relay peers in the trusted relay "trusted"
            // we could consider adding blocks with no consensus verification (only state advancements), to further speed up the synchronization of fast relay nodes.
            // at least this could reduce the latency for block templating.
            let BlockValidationFutures { block_task, virtual_state_task } = session.validate_and_insert_block(block.clone());

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
                }
            };

            info!("Block {} from fast relay passed validation", hash);

            if broadcast {
                self.ctx
                    .hub()
                    .broadcast(
                        make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(hash.into()) }),
                        None, // Because of fast relay block fragmentation, we don't consider one "particular" peer to have sent us this.
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
