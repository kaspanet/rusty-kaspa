use kaspa_hashes::Hash;

pub trait SeqCommitAccessor: Sync {
    fn is_chain_ancestor_from_pov(&self, block_hash: Hash) -> Option<bool>;
    fn seq_commitment_within_depth(&self, block_hash: Hash) -> Option<Hash>;
}
