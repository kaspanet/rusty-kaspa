//! Consensus and Session management structures.
//!
//! We use newtypes in order to simplify changing the underlying lock in the future

use kaspa_consensus_core::{
    acceptance_data::AcceptanceData,
    api::{BlockCount, BlockValidationFutures, ConsensusApi, ConsensusStats, DynConsensus},
    block::Block,
    blockstatus::BlockStatus,
    daa_score_timestamp::DaaScoreTimestamp,
    errors::consensus::ConsensusResult,
    header::Header,
    pruning::{PruningPointProof, PruningPointTrustedData, PruningPointsList},
    trusted::{ExternalGhostdagData, TrustedBlock},
    tx::{MutableTransaction, Transaction, TransactionOutpoint, UtxoEntry},
    BlockHashSet, BlueWorkType, ChainPath, Hash,
};
use kaspa_utils::sync::rwlock::*;
use std::{ops::Deref, sync::Arc};

pub use tokio::task::spawn_blocking;

use crate::BlockProcessingBatch;

#[allow(dead_code)]
#[derive(Clone)]
pub struct SessionOwnedReadGuard(Arc<RfRwLockOwnedReadGuard>);

#[allow(dead_code)]
pub struct SessionReadGuard<'a>(RfRwLockReadGuard<'a>);

pub struct SessionWriteGuard<'a>(RfRwLockWriteGuard<'a>);

impl SessionWriteGuard<'_> {
    /// Releases and recaptures the write lock. Makes sure that other pending readers/writers get a
    /// chance to capture the lock before this thread does so.
    pub fn blocking_yield(&mut self) {
        self.0.blocking_yield();
    }
}

#[derive(Clone)]
pub struct SessionLock(Arc<RfRwLock>);

impl Default for SessionLock {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionLock {
    pub fn new() -> SessionLock {
        SessionLock(Arc::new(RfRwLock::new()))
    }

    pub async fn read_owned(&self) -> SessionOwnedReadGuard {
        SessionOwnedReadGuard(Arc::new(self.0.clone().read_owned().await))
    }

    pub async fn read(&self) -> SessionReadGuard {
        SessionReadGuard(self.0.read().await)
    }

    pub fn blocking_read(&self) -> SessionReadGuard {
        SessionReadGuard(self.0.blocking_read())
    }

    pub fn blocking_write(&self) -> SessionWriteGuard<'_> {
        SessionWriteGuard(self.0.blocking_write())
    }
}

#[derive(Clone)]
pub struct ConsensusInstance {
    session_lock: SessionLock,
    consensus: DynConsensus,
}

impl ConsensusInstance {
    pub fn new(session_lock: SessionLock, consensus: DynConsensus) -> Self {
        Self { session_lock, consensus }
    }

    /// Returns a blocking session to be used in **non async** environments.
    /// Users would usually need to call something like `futures::executor::block_on` in order
    /// to acquire the session, but we prefer leaving this decision to the caller
    pub async fn session_blocking(&self) -> ConsensusSessionBlocking {
        let g = self.session_lock.read().await;
        ConsensusSessionBlocking::new(g, self.consensus.clone())
    }

    /// Returns an unguarded *blocking* consensus session. There's no guarantee that data will not be pruned between
    /// two sequential consensus calls. This session doesn't hold the consensus pruning lock, so it should
    /// be preferred upon [`session_blocking`] when data consistency is not important.
    pub fn unguarded_session_blocking(&self) -> ConsensusSessionBlocking<'static> {
        ConsensusSessionBlocking::new_without_session_guard(self.consensus.clone())
    }

    /// Returns a consensus session for accessing consensus operations in a bulk. The user can safely assume
    /// that consensus state is consistent between operations, that is, no pruning was performed between the calls.
    /// The returned object is an *owned* consensus session type which can be cloned and shared across threads.
    /// The sharing ability is useful for spawning blocking operations on a different thread using the same
    /// session object, see [`ConsensusSessionOwned::spawn_blocking`]. The caller is responsible to make sure
    /// that the overall lifetime of this session is not too long (~2 seconds max)
    pub async fn session(&self) -> ConsensusSessionOwned {
        let g = self.session_lock.read_owned().await;
        ConsensusSessionOwned::new(g, self.consensus.clone())
    }

    /// Returns an unguarded consensus session. There's no guarantee that data will not be pruned between
    /// two sequential consensus calls. This session doesn't hold the consensus pruning lock, so it should
    /// be preferred upon [`session`] when data consistency is not important.
    pub fn unguarded_session(&self) -> ConsensusSessionOwned {
        ConsensusSessionOwned::new_without_session_guard(self.consensus.clone())
    }
}

pub struct ConsensusSessionBlocking<'a> {
    _session_guard: Option<SessionReadGuard<'a>>,
    consensus: DynConsensus,
}

impl<'a> ConsensusSessionBlocking<'a> {
    pub fn new(session_guard: SessionReadGuard<'a>, consensus: DynConsensus) -> Self {
        Self { _session_guard: Some(session_guard), consensus }
    }

    pub fn new_without_session_guard(consensus: DynConsensus) -> Self {
        Self { _session_guard: None, consensus }
    }
}

impl Deref for ConsensusSessionBlocking<'_> {
    type Target = dyn ConsensusApi; // We avoid exposing the Arc itself by ref since it can be easily cloned and misused

    fn deref(&self) -> &Self::Target {
        self.consensus.as_ref()
    }
}

/// An *owned* consensus session type which can be cloned and shared across threads.
/// See method `spawn_blocking` within for context on the usefulness of this type
#[derive(Clone)]
pub struct ConsensusSessionOwned {
    _session_guard: Option<SessionOwnedReadGuard>,
    consensus: DynConsensus,
}

impl ConsensusSessionOwned {
    pub fn new(session_guard: SessionOwnedReadGuard, consensus: DynConsensus) -> Self {
        Self { _session_guard: Some(session_guard), consensus }
    }

    pub fn new_without_session_guard(consensus: DynConsensus) -> Self {
        Self { _session_guard: None, consensus }
    }

    /// Uses [`tokio::task::spawn_blocking`] to run the provided consensus closure on a thread where blocking is acceptable.
    /// Note that this function is only available on the *owned* session, and requires cloning the session. In fact this
    /// function is the main motivation for a separate session type.
    pub async fn spawn_blocking<F, R>(self, f: F) -> R
    where
        F: FnOnce(&dyn ConsensusApi) -> R + Send + 'static,
        R: Send + 'static,
    {
        spawn_blocking(move || f(self.consensus.as_ref())).await.unwrap()
    }
}

impl ConsensusSessionOwned {
    pub fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
        self.consensus.validate_and_insert_block(block)
    }

    pub fn validate_and_insert_block_batch(&self, mut batch: Vec<Block>) -> BlockProcessingBatch {
        // Sort by blue work in order to ensure topological order
        batch.sort_by(|a, b| a.header.blue_work.partial_cmp(&b.header.blue_work).unwrap());
        let (block_tasks, virtual_state_tasks) = batch
            .iter()
            .map(|b| {
                let BlockValidationFutures { block_task, virtual_state_task } = self.consensus.validate_and_insert_block(b.clone());
                (block_task, virtual_state_task)
            })
            .unzip();
        BlockProcessingBatch::new(batch, block_tasks, virtual_state_tasks)
    }

    pub fn validate_and_insert_trusted_block(&self, tb: TrustedBlock) -> BlockValidationFutures {
        self.consensus.validate_and_insert_trusted_block(tb)
    }

    pub fn calculate_transaction_compute_mass(&self, transaction: &Transaction) -> u64 {
        // This method performs pure calculations so no need for an async wrapper
        self.consensus.calculate_transaction_compute_mass(transaction)
    }

    pub fn calculate_transaction_storage_mass(&self, transaction: &MutableTransaction) -> Option<u64> {
        // This method performs pure calculations so no need for an async wrapper
        self.consensus.calculate_transaction_storage_mass(transaction)
    }

    pub fn get_virtual_daa_score(&self) -> u64 {
        // Accessing cached virtual fields is lock-free and does not require spawn_blocking
        self.consensus.get_virtual_daa_score()
    }

    pub fn get_virtual_bits(&self) -> u32 {
        // Accessing cached virtual fields is lock-free and does not require spawn_blocking
        self.consensus.get_virtual_bits()
    }

    pub fn get_virtual_past_median_time(&self) -> u64 {
        // Accessing cached virtual fields is lock-free and does not require spawn_blocking
        self.consensus.get_virtual_past_median_time()
    }

    pub fn get_virtual_parents(&self) -> BlockHashSet {
        // Accessing cached virtual fields is lock-free and does not require spawn_blocking
        self.consensus.get_virtual_parents()
    }

    pub fn get_virtual_parents_len(&self) -> usize {
        // Accessing cached virtual fields is lock-free and does not require spawn_blocking
        self.consensus.get_virtual_parents_len()
    }

    pub async fn async_get_stats(&self) -> ConsensusStats {
        self.clone().spawn_blocking(|c| c.get_stats()).await
    }

    pub async fn async_get_virtual_merge_depth_root(&self) -> Option<Hash> {
        self.clone().spawn_blocking(|c| c.get_virtual_merge_depth_root()).await
    }

    /// Returns the `BlueWork` threshold at which blocks with lower or equal blue work are considered
    /// to be un-mergeable by current virtual state.
    /// (Note: in some rare cases when the node is unsynced the function might return zero as the threshold)
    pub async fn async_get_virtual_merge_depth_blue_work_threshold(&self) -> BlueWorkType {
        self.clone().spawn_blocking(|c| c.get_virtual_merge_depth_blue_work_threshold()).await
    }

    pub async fn async_get_sink(&self) -> Hash {
        self.clone().spawn_blocking(|c| c.get_sink()).await
    }

    pub async fn async_get_sink_timestamp(&self) -> u64 {
        self.clone().spawn_blocking(|c| c.get_sink_timestamp()).await
    }

    pub async fn async_get_current_block_color(&self, hash: Hash) -> Option<bool> {
        self.clone().spawn_blocking(move |c| c.get_current_block_color(hash)).await
    }

    /// source refers to the earliest block from which the current node has full header & block data  
    pub async fn async_get_source(&self) -> Hash {
        self.clone().spawn_blocking(|c| c.get_source()).await
    }

    pub async fn async_estimate_block_count(&self) -> BlockCount {
        self.clone().spawn_blocking(|c| c.estimate_block_count()).await
    }

    /// Returns whether this consensus is considered synced or close to being synced.
    ///
    /// This info is used to determine if it's ok to use a block template from this node for mining purposes.
    pub async fn async_is_nearly_synced(&self) -> bool {
        self.clone().spawn_blocking(|c| c.is_nearly_synced()).await
    }

    pub async fn async_get_virtual_chain_from_block(&self, hash: Hash) -> ConsensusResult<ChainPath> {
        self.clone().spawn_blocking(move |c| c.get_virtual_chain_from_block(hash)).await
    }

    pub async fn async_get_virtual_utxos(
        &self,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> Vec<(TransactionOutpoint, UtxoEntry)> {
        self.clone().spawn_blocking(move |c| c.get_virtual_utxos(from_outpoint, chunk_size, skip_first)).await
    }

    pub async fn async_get_tips(&self) -> Vec<Hash> {
        self.clone().spawn_blocking(|c| c.get_tips()).await
    }

    pub async fn async_get_tips_len(&self) -> usize {
        self.clone().spawn_blocking(|c| c.get_tips_len()).await
    }

    pub async fn async_is_chain_ancestor_of(&self, low: Hash, high: Hash) -> ConsensusResult<bool> {
        self.clone().spawn_blocking(move |c| c.is_chain_ancestor_of(low, high)).await
    }

    pub async fn async_get_hashes_between(&self, low: Hash, high: Hash, max_blocks: usize) -> ConsensusResult<(Vec<Hash>, Hash)> {
        self.clone().spawn_blocking(move |c| c.get_hashes_between(low, high, max_blocks)).await
    }

    pub async fn async_get_header(&self, hash: Hash) -> ConsensusResult<Arc<Header>> {
        self.clone().spawn_blocking(move |c| c.get_header(hash)).await
    }

    pub async fn async_get_headers_selected_tip(&self) -> Hash {
        self.clone().spawn_blocking(|c| c.get_headers_selected_tip()).await
    }

    pub async fn async_get_chain_block_samples(&self) -> Vec<DaaScoreTimestamp> {
        self.clone().spawn_blocking(|c| c.get_chain_block_samples()).await
    }

    /// Returns the antipast of block `hash` from the POV of `context`, i.e. `antipast(hash) ∩ past(context)`.
    /// Since this might be an expensive operation for deep blocks, we allow the caller to specify a limit
    /// `max_traversal_allowed` on the maximum amount of blocks to traverse for obtaining the answer
    pub async fn async_get_antipast_from_pov(
        &self,
        hash: Hash,
        context: Hash,
        max_traversal_allowed: Option<u64>,
    ) -> ConsensusResult<Vec<Hash>> {
        self.clone().spawn_blocking(move |c| c.get_antipast_from_pov(hash, context, max_traversal_allowed)).await
    }

    /// Returns the anticone of block `hash` from the POV of `virtual`
    pub async fn async_get_anticone(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        self.clone().spawn_blocking(move |c| c.get_anticone(hash)).await
    }

    pub async fn async_get_pruning_point_proof(&self) -> Arc<PruningPointProof> {
        self.clone().spawn_blocking(|c| c.get_pruning_point_proof()).await
    }

    pub async fn async_create_virtual_selected_chain_block_locator(
        &self,
        low: Option<Hash>,
        high: Option<Hash>,
    ) -> ConsensusResult<Vec<Hash>> {
        self.clone().spawn_blocking(move |c| c.create_virtual_selected_chain_block_locator(low, high)).await
    }

    pub async fn async_create_block_locator_from_pruning_point(&self, high: Hash, limit: usize) -> ConsensusResult<Vec<Hash>> {
        self.clone().spawn_blocking(move |c| c.create_block_locator_from_pruning_point(high, limit)).await
    }

    pub async fn async_pruning_point_headers(&self) -> Vec<Arc<Header>> {
        self.clone().spawn_blocking(|c| c.pruning_point_headers()).await
    }

    pub async fn async_get_pruning_point_anticone_and_trusted_data(&self) -> ConsensusResult<Arc<PruningPointTrustedData>> {
        self.clone().spawn_blocking(|c| c.get_pruning_point_anticone_and_trusted_data()).await
    }

    pub async fn async_get_block(&self, hash: Hash) -> ConsensusResult<Block> {
        self.clone().spawn_blocking(move |c| c.get_block(hash)).await
    }

    pub async fn async_get_block_even_if_header_only(&self, hash: Hash) -> ConsensusResult<Block> {
        self.clone().spawn_blocking(move |c| c.get_block_even_if_header_only(hash)).await
    }

    pub async fn async_get_ghostdag_data(&self, hash: Hash) -> ConsensusResult<ExternalGhostdagData> {
        self.clone().spawn_blocking(move |c| c.get_ghostdag_data(hash)).await
    }

    pub async fn async_get_block_children(&self, hash: Hash) -> Option<Vec<Hash>> {
        self.clone().spawn_blocking(move |c| c.get_block_children(hash)).await
    }

    pub async fn async_get_block_parents(&self, hash: Hash) -> Option<Arc<Vec<Hash>>> {
        self.clone().spawn_blocking(move |c| c.get_block_parents(hash)).await
    }

    pub async fn async_get_block_status(&self, hash: Hash) -> Option<BlockStatus> {
        self.clone().spawn_blocking(move |c| c.get_block_status(hash)).await
    }

    pub async fn async_get_block_acceptance_data(&self, hash: Hash) -> ConsensusResult<Arc<AcceptanceData>> {
        self.clone().spawn_blocking(move |c| c.get_block_acceptance_data(hash)).await
    }

    /// Returns acceptance data for a set of blocks belonging to the selected parent chain.
    ///
    /// See `self::get_virtual_chain`
    pub async fn async_get_blocks_acceptance_data(&self, hashes: Vec<Hash>) -> ConsensusResult<Vec<Arc<AcceptanceData>>> {
        self.clone().spawn_blocking(move |c| c.get_blocks_acceptance_data(&hashes)).await
    }

    pub async fn async_is_chain_block(&self, hash: Hash) -> ConsensusResult<bool> {
        self.clone().spawn_blocking(move |c| c.is_chain_block(hash)).await
    }

    pub async fn async_get_pruning_point_utxos(
        &self,
        expected_pruning_point: Hash,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> ConsensusResult<Vec<(TransactionOutpoint, UtxoEntry)>> {
        self.clone()
            .spawn_blocking(move |c| c.get_pruning_point_utxos(expected_pruning_point, from_outpoint, chunk_size, skip_first))
            .await
    }

    pub async fn async_get_missing_block_body_hashes(&self, high: Hash) -> ConsensusResult<Vec<Hash>> {
        self.clone().spawn_blocking(move |c| c.get_missing_block_body_hashes(high)).await
    }

    pub async fn async_pruning_point(&self) -> Hash {
        self.clone().spawn_blocking(|c| c.pruning_point()).await
    }

    pub async fn async_get_daa_window(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        self.clone().spawn_blocking(move |c| c.get_daa_window(hash)).await
    }

    pub async fn async_get_trusted_block_associated_ghostdag_data_block_hashes(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        self.clone().spawn_blocking(move |c| c.get_trusted_block_associated_ghostdag_data_block_hashes(hash)).await
    }

    pub async fn async_estimate_network_hashes_per_second(
        &self,
        start_hash: Option<Hash>,
        window_size: usize,
    ) -> ConsensusResult<u64> {
        self.clone().spawn_blocking(move |c| c.estimate_network_hashes_per_second(start_hash, window_size)).await
    }

    pub async fn async_validate_pruning_points(&self) -> ConsensusResult<()> {
        self.clone().spawn_blocking(move |c| c.validate_pruning_points()).await
    }

    pub async fn async_are_pruning_points_violating_finality(&self, pp_list: PruningPointsList) -> bool {
        self.clone().spawn_blocking(move |c| c.are_pruning_points_violating_finality(pp_list)).await
    }

    pub async fn async_creation_timestamp(&self) -> u64 {
        self.clone().spawn_blocking(move |c| c.creation_timestamp()).await
    }

    pub async fn async_finality_point(&self) -> Hash {
        self.clone().spawn_blocking(move |c| c.finality_point()).await
    }
}

pub type ConsensusProxy = ConsensusSessionOwned;
