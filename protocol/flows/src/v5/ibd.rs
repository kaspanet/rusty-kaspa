use crate::ctx::FlowContext;
use consensus_core::{
    api::DynConsensus,
    block::Block,
    ghostdag::{TrustedBlock, TrustedDataEntry, TrustedDataPackage},
    pruning::{PruningPointProof, PruningPointsList},
    BlockHashMap, BlockHashSet, HashMapCustomHasher,
};
use kaspa_core::{debug, info};
use p2p_lib::{
    common::FlowError,
    dequeue_with_timeout, make_message,
    pb::{
        kaspad_message::Payload, RequestNextPruningPointAndItsAnticoneBlocksMessage, RequestPruningPointAndItsAnticoneMessage,
        RequestPruningPointProofMessage,
    },
    IncomingRoute, Router,
};
use std::{sync::Arc, time::Duration};

/// Flow for managing IBD - Initial Block Download
pub struct IbdFlow {
    ctx: FlowContext,
    pub router: Arc<Router>, // TODO: remove pub
    incoming_route: IncomingRoute,
}

impl IbdFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    pub async fn start(&mut self) -> Result<(), FlowError> {
        // TEMP
        // TODO: start flows only after ready flow runs
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.start_ibd_with_headers_proof().await?;
        Ok(())
    }

    async fn start_ibd_with_headers_proof(&mut self) -> Result<(), FlowError> {
        info!("Starting IBD with headers proof");
        let consensus = self.ctx.consensus();
        self.sync_and_validate_pruning_proof(&consensus).await?;
        Ok(())
    }

    async fn sync_and_validate_pruning_proof(&mut self, consensus: &DynConsensus) -> Result<(), FlowError> {
        self.router.enqueue(make_message!(Payload::RequestPruningPointProof, RequestPruningPointProofMessage {})).await?;

        // Pruning proof generation and communication might take several minutes, so we allow a long 10 minute timeout
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointProof, Duration::from_secs(600))?;
        let proof: PruningPointProof = msg.try_into()?;
        debug!("received proof with overall {} headers", proof.iter().map(|l| l.len()).sum::<usize>());

        // TODO: call validate_pruning_proof when implemented
        // consensus.clone().validate_pruning_proof(&proof);

        let _proof_pruning_point = proof[0].last().expect("was just insured by validation").hash;

        // TODO: verify the proof pruning point is different than current consensus pruning point

        self.router
            .enqueue(make_message!(Payload::RequestPruningPointAndItsAnticone, RequestPruningPointAndItsAnticoneMessage {}))
            .await?;

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPoints)?;
        let _pruning_points: PruningPointsList = msg.try_into()?;

        // TODO: verify last pruning point header hashes to proof_pruning_point
        // TODO: import pruning points into consensus

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::TrustedData)?;
        let pkg: TrustedDataPackage = msg.try_into()?;
        debug!("received trusted data with {} daa entries and {} ghostdag entries", pkg.daa_window.len(), pkg.ghostdag_window.len());

        let mut entry_stream = TrustedEntryStream::new(&self.router, &mut self.incoming_route);
        let Some(pruning_point_entry) = entry_stream.next().await? else { return Err(FlowError::ProtocolError("got `done` message before receiving the pruning point")); };

        // TODO: verify trusted pruning point matches proof pruning point

        let mut entries = vec![pruning_point_entry];
        while let Some(entry) = entry_stream.next().await? {
            entries.push(entry);
        }

        let trusted_set = build_trusted_set(pkg, entries)?;
        consensus.clone().apply_pruning_proof(proof, &trusted_set);

        Ok(())
    }
}

fn build_trusted_set(pkg: TrustedDataPackage, entries: Vec<TrustedDataEntry>) -> Result<Vec<TrustedBlock>, FlowError> {
    let mut blocks = Vec::with_capacity(entries.len());
    let mut set = BlockHashSet::new();
    let mut map = BlockHashMap::new();

    for th in pkg.ghostdag_window.iter() {
        map.insert(th.hash, th.ghostdag.clone());
    }

    for th in pkg.daa_window.iter() {
        map.insert(th.header.hash, th.ghostdag.clone());
    }

    for entry in entries {
        let block = entry.block;
        if set.insert(block.hash()) {
            if let Some(ghostdag) = map.get(&block.hash()) {
                blocks.push(TrustedBlock::new(block, ghostdag.clone()));
            } else {
                return Err(FlowError::ProtocolError("missing ghostdag data for some trusted entries"));
            }
        }
    }

    for th in pkg.daa_window.iter() {
        if set.insert(th.header.hash) {
            blocks.push(TrustedBlock::new(Block::from_header_arc(th.header.clone()), th.ghostdag.clone()));
        }
    }

    // Topological sort
    blocks.sort_by(|a, b| a.block.header.blue_work.cmp(&b.block.header.blue_work));

    Ok(blocks)
}

const IBD_BATCH_SIZE: usize = 99;

pub struct TrustedEntryStream<'a, 'b> {
    router: &'a Router,
    incoming_route: &'b mut IncomingRoute,
    i: usize,
}

impl<'a, 'b> TrustedEntryStream<'a, 'b> {
    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute) -> Self {
        Self { router, incoming_route, i: 0 }
    }

    pub async fn next(&mut self) -> Result<Option<TrustedDataEntry>, FlowError> {
        let msg = match tokio::time::timeout(p2p_lib::common::DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::BlockWithTrustedDataV4(payload)) => Ok(Some(payload.try_into()?)),
                        Some(Payload::DoneBlocksWithTrustedData(_)) => {
                            debug!("trusted entry stream completed after {} items", self.i);
                            return Ok(None);
                        }
                        _ => Err(FlowError::UnexpectedMessageType(
                            stringify!(Payload::BlockWithTrustedDataV4 | Payload::DoneBlocksWithTrustedData),
                            Box::new(msg.payload),
                        )),
                    }
                } else {
                    Err(FlowError::P2pConnectionError(p2p_lib::ConnectionError::ChannelClosed))
                }
            }
            Err(_) => Err(FlowError::Timeout(p2p_lib::common::DEFAULT_TIMEOUT)),
        };

        // Request the next batch
        // TODO: test that batch counting is correct and follows golang imp
        self.i += 1;
        if self.i % IBD_BATCH_SIZE == 0 {
            info!("Downloaded {} blocks from the pruning point anticone", self.i - 1);
            self.router
                .enqueue(make_message!(
                    Payload::RequestNextPruningPointAndItsAnticoneBlocks,
                    RequestNextPruningPointAndItsAnticoneBlocksMessage {}
                ))
                .await?;
        }

        msg
    }
}
