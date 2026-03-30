use kaspa_hashes::Hash;

/// Wrapper for data retrieved via historical seek that may be from a fork.
///
/// Carries the `blue_score` and `block_hash` from the version key so callers
/// can verify canonicality via the reachability service before trusting
/// the data.
pub struct MaybeFork<T> {
    data: T,
    blue_score: u64,
    block_hash: Hash,
}

impl<T> MaybeFork<T> {
    pub fn new(data: T, blue_score: u64, block_hash: Hash) -> Self {
        Self { data, blue_score, block_hash }
    }

    pub fn block_hash(&self) -> Hash {
        self.block_hash
    }

    pub fn blue_score(&self) -> u64 {
        self.blue_score
    }

    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn into_parts(self) -> (T, u64, Hash) {
        (self.data, self.blue_score, self.block_hash)
    }

    /// Convert into a `Verified` after confirming canonicality.
    pub fn into_verified(self) -> Verified<T> {
        Verified { data: self.data, blue_score: self.blue_score, block_hash: self.block_hash }
    }
}

/// Version entry whose canonicality has been verified.
///
/// Returned by `get` methods that accept an `is_canonical` predicate.
pub struct Verified<T> {
    data: T,
    blue_score: u64,
    block_hash: Hash,
}

impl<T> Verified<T> {
    pub fn new(data: T, blue_score: u64, block_hash: Hash) -> Self {
        Self { data, blue_score, block_hash }
    }

    pub fn block_hash(&self) -> Hash {
        self.block_hash
    }

    pub fn blue_score(&self) -> u64 {
        self.blue_score
    }

    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn into_parts(self) -> (T, u64, Hash) {
        (self.data, self.blue_score, self.block_hash)
    }
}
