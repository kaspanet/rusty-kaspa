use derive_more::Display;
use kaspa_consensus_core::{acceptance_data::AcceptanceData, block::Block, utxo::utxo_diff::UtxoDiff};
use kaspa_hashes::Hash;
use kaspa_notify::{
    events::EventType,
    full_featured,
    notification::Notification as NotificationTrait,
    subscription::{
        single::{OverallSubscription, UtxosChangedSubscription, VirtualChainChangedSubscription},
        Subscription,
    },
};
use std::sync::Arc;

full_featured! {
#[derive(Clone, Debug, Display)]
pub enum Notification {
    #[display(fmt = "BlockAdded notification: block hash {}", "_0.block.header.hash")]
    BlockAdded(BlockAddedNotification),

    #[display(fmt = "VirtualChainChanged notification: {} removed blocks, {} added blocks, {} accepted transactions", "_0.removed_chain_block_hashes.len()", "_0.added_chain_block_hashes.len()", "_0.added_chain_blocks_acceptance_data.len()")]
    VirtualChainChanged(VirtualChainChangedNotification),

    #[display(fmt = "FinalityConflict notification: violating block hash {}", "_0.violating_block_hash")]
    FinalityConflict(FinalityConflictNotification),

    #[display(fmt = "FinalityConflict notification: violating block hash {}", "_0.finality_block_hash")]
    FinalityConflictResolved(FinalityConflictResolvedNotification),

    #[display(fmt = "UtxosChanged notification")]
    UtxosChanged(UtxosChangedNotification),

    #[display(fmt = "SinkBlueScoreChanged notification: virtual selected parent blue score {}", "_0.sink_blue_score")]
    SinkBlueScoreChanged(SinkBlueScoreChangedNotification),

    #[display(fmt = "VirtualDaaScoreChanged notification: virtual DAA score {}", "_0.virtual_daa_score")]
    VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification),

    #[display(fmt = "PruningPointUtxoSetOverride notification")]
    PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification),

    #[display(fmt = "NewBlockTemplate notification")]
    NewBlockTemplate(NewBlockTemplateNotification),
    #[display(fmt = "SyncStateChanged notification")]
    SyncStateChanged(SyncStateChangedNotification),
}
}

impl NotificationTrait for Notification {
    fn apply_overall_subscription(&self, subscription: &OverallSubscription) -> Option<Self> {
        match subscription.active() {
            true => Some(self.clone()),
            false => None,
        }
    }

    fn apply_virtual_chain_changed_subscription(&self, subscription: &VirtualChainChangedSubscription) -> Option<Self> {
        match subscription.active() {
            true => {
                // If the subscription excludes accepted transaction ids and the notification includes some
                // then we must re-create the object and drop the ids, otherwise we can clone it as is.
                if let Notification::VirtualChainChanged(ref payload) = self {
                    if !subscription.include_accepted_transaction_ids() && !payload.added_chain_blocks_acceptance_data.is_empty() {
                        return Some(Notification::VirtualChainChanged(VirtualChainChangedNotification {
                            removed_chain_block_hashes: payload.removed_chain_block_hashes.clone(),
                            added_chain_block_hashes: payload.added_chain_block_hashes.clone(),
                            added_chain_blocks_acceptance_data: Arc::new(vec![]),
                        }));
                    }
                }
                Some(self.clone())
            }
            false => None,
        }
    }

    fn apply_utxos_changed_subscription(&self, _subscription: &UtxosChangedSubscription) -> Option<Self> {
        // No effort is made here to apply the subscription addresses.
        // This will be achieved farther along the notification backbone.
        Some(self.clone())
    }

    fn event_type(&self) -> EventType {
        self.into()
    }
}

#[derive(Debug, Clone)]
pub struct BlockAddedNotification {
    pub block: Block,
}

impl BlockAddedNotification {
    pub fn new(block: Block) -> Self {
        Self { block }
    }
}

#[derive(Debug, Clone)]
pub struct VirtualChainChangedNotification {
    pub added_chain_block_hashes: Arc<Vec<Hash>>,
    pub removed_chain_block_hashes: Arc<Vec<Hash>>,
    pub added_chain_blocks_acceptance_data: Arc<Vec<Arc<AcceptanceData>>>,
}
impl VirtualChainChangedNotification {
    pub fn new(
        added_chain_block_hashes: Arc<Vec<Hash>>,
        removed_chain_block_hashes: Arc<Vec<Hash>>,
        added_chain_blocks_acceptance_data: Arc<Vec<Arc<AcceptanceData>>>,
    ) -> Self {
        Self { added_chain_block_hashes, removed_chain_block_hashes, added_chain_blocks_acceptance_data }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FinalityConflictNotification {
    pub violating_block_hash: Hash,
}

impl FinalityConflictNotification {
    pub fn new(violating_block_hash: Hash) -> Self {
        Self { violating_block_hash }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FinalityConflictResolvedNotification {
    pub finality_block_hash: Hash,
}

impl FinalityConflictResolvedNotification {
    pub fn new(finality_block_hash: Hash) -> Self {
        Self { finality_block_hash }
    }
}

#[derive(Debug, Clone)]
pub struct UtxosChangedNotification {
    /// Accumulated UTXO diff between the last virtual state and the current virtual state
    pub accumulated_utxo_diff: Arc<UtxoDiff>,
    pub virtual_parents: Arc<Vec<Hash>>,
}

impl UtxosChangedNotification {
    pub fn new(accumulated_utxo_diff: Arc<UtxoDiff>, virtual_parents: Arc<Vec<Hash>>) -> Self {
        Self { accumulated_utxo_diff, virtual_parents }
    }
}

#[derive(Debug, Clone)]
pub struct SinkBlueScoreChangedNotification {
    pub sink_blue_score: u64,
}

impl SinkBlueScoreChangedNotification {
    pub fn new(sink_blue_score: u64) -> Self {
        Self { sink_blue_score }
    }
}

#[derive(Debug, Clone)]
pub struct VirtualDaaScoreChangedNotification {
    pub virtual_daa_score: u64,
}

impl VirtualDaaScoreChangedNotification {
    pub fn new(virtual_daa_score: u64) -> Self {
        Self { virtual_daa_score }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PruningPointUtxoSetOverrideNotification {}

#[derive(Debug, Clone)]
pub struct NewBlockTemplateNotification {}

#[derive(Debug, Clone)]
pub enum SyncStateChangedNotification {
    Proof { current: u8, max: u8 },
    Headers { headers: u64, progress: i64 },
    Blocks { blocks: u64, progress: i64 },
    UtxoResync,
    UtxoSync { chunks: u64, total: u64 },
    TrustSync { processed: u64, total: u64 },
    Synced,
}

impl SyncStateChangedNotification {
    pub fn new_proof(current: u8, max: u8) -> Self {
        Self::Proof { current, max }
    }

    pub fn new_headers(headers: u64, progress: i64) -> Self {
        Self::Headers { headers, progress }
    }

    pub fn new_blocks(blocks: u64, progress: i64) -> Self {
        Self::Blocks { blocks, progress }
    }

    pub fn new_utxo_resync() -> Self {
        Self::UtxoResync
    }

    pub fn new_utxo_sync(chunks: u64, total: u64) -> Self {
        Self::UtxoSync { chunks, total }
    }

    pub fn new_trust_sync(processed: u64, total: u64) -> Self {
        Self::TrustSync { processed, total }
    }

    pub fn new_synced() -> Self {
        Self::Synced
    }
}
