use crate::model::services::reachability::{MTReachabilityService, ReachabilityService};
use crate::model::stores::headers::{DbHeadersStore, HeaderStoreReader};
use crate::model::stores::reachability::DbReachabilityStore;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_database::prelude::StoreResultExt;
use kaspa_hashes::Hash;

/// Shared utility function to keep the "within threshold" definition consistent across callers.
pub fn seq_commit_within_threshold(high_blue_score: u64, low_blue_score: u64, threshold: u64) -> bool {
    low_blue_score + threshold > high_blue_score
}

#[derive(Copy, Clone)]
pub struct SeqCommitAccessor<'a> {
    pub threshold: u64,
    pub sp: Hash,
    pub reachability_service: &'a MTReachabilityService<DbReachabilityStore>,
    pub headers_store: &'a DbHeadersStore,
    pub covenants_activation: ForkActivation,
}

impl<'a> SeqCommitAccessor<'a> {
    pub fn new(
        sp: Hash,
        reachability_service: &'a MTReachabilityService<DbReachabilityStore>,
        headers_store: &'a DbHeadersStore,
        covenants_activation: ForkActivation,
        threshold: u64,
    ) -> Self {
        Self { threshold, sp, reachability_service, headers_store, covenants_activation }
    }
}

impl<'a> kaspa_txscript::SeqCommitAccessor for SeqCommitAccessor<'a> {
    fn is_chain_ancestor_from_pov(&self, block_hash: Hash) -> Option<bool> {
        self.reachability_service.try_is_chain_ancestor_of(block_hash, self.sp).optional().unwrap()
    }

    fn seq_commitment_within_depth(&self, block_hash: Hash) -> Option<Hash> {
        let header = self.headers_store.get_header(block_hash).optional().unwrap()?;
        if !self.covenants_activation.is_active(header.daa_score) {
            return None;
        }
        let sp_blue_score = self.headers_store.get_blue_score(self.sp).unwrap();
        if seq_commit_within_threshold(sp_blue_score, header.blue_score, self.threshold) {
            Some(header.accepted_id_merkle_root)
        } else {
            None
        }
    }
}
