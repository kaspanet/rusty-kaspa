use kaspa_hashes::Hash;
use zerocopy::byteorder::little_endian::U32 as U32LE;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use crate::reverse_blue_score::ReverseBlueScore;

/// Branch version key.
///
/// Layout: `prefix(1) | depth(1) | node_key(32) | rev_blue_score(8) | block_hash(32)` = 74 bytes.
/// Uses [`ReverseBlueScore`] so forward iteration yields latest versions first.
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy)]
#[repr(C)]
pub struct BranchVersionKey {
    pub prefix: u8,
    pub depth: u8,
    pub node_key: Hash,
    pub rev_blue_score: ReverseBlueScore,
    pub block_hash: Hash,
}

impl BranchVersionKey {
    pub fn new(prefix: u8, depth: u8, node_key: Hash, blue_score: u64, block_hash: Hash) -> Self {
        Self { prefix, depth, node_key, rev_blue_score: ReverseBlueScore::new(blue_score), block_hash }
    }

    /// Build a seek key for finding versions at or before `target_blue_score`.
    /// block_hash is zeroed so the seek lands at the start of that score.
    pub fn seek_key(prefix: u8, depth: u8, node_key: Hash, target_blue_score: u64) -> Self {
        Self {
            prefix,
            depth,
            node_key,
            rev_blue_score: ReverseBlueScore::new(target_blue_score),
            block_hash: Hash::from_bytes([0; 32]),
        }
    }

    /// Entity prefix: `prefix(1) | depth(1) | node_key(32)` = 34 bytes.
    pub const ENTITY_PREFIX_LEN: usize = 1 + 1 + 32;
}

impl AsRef<[u8]> for BranchVersionKey {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// Lane version key.
///
/// Layout: `prefix(1) | lane_key(32) | rev_blue_score(8) | block_hash(32)` = 73 bytes.
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy)]
#[repr(C)]
pub struct LaneVersionKey {
    pub prefix: u8,
    pub lane_key: Hash,
    pub rev_blue_score: ReverseBlueScore,
    pub block_hash: Hash,
}

impl LaneVersionKey {
    pub fn new(prefix: u8, lane_key: Hash, blue_score: u64, block_hash: Hash) -> Self {
        Self { prefix, lane_key, rev_blue_score: ReverseBlueScore::new(blue_score), block_hash }
    }

    pub fn seek_key(prefix: u8, lane_key: Hash, target_blue_score: u64) -> Self {
        Self { prefix, lane_key, rev_blue_score: ReverseBlueScore::new(target_blue_score), block_hash: Hash::from_bytes([0; 32]) }
    }

    /// Entity prefix: `prefix(1) | lane_key(32)` = 33 bytes.
    pub const ENTITY_PREFIX_LEN: usize = 1 + 32;
}

impl AsRef<[u8]> for LaneVersionKey {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// Distinguishes score index entry kinds.
///
/// - `LeafUpdate` (0): a lane was inserted or updated at this score.
///   Keyed by the lane's own blue_score. Used by expiration logic to find stale lanes,
///   and by pruning to delete lane_version + branch_version entries.
/// - `Structural` (1): tree structure changed at this score (expiration, collapsed node merge/split).
///   Keyed by the block's blue_score (or pruning point blue_score during IBD).
///   Used by pruning to delete branch_version entries only.
#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ScoreIndexKind {
    /// Lane inserted or updated. Score = lane's blue_score.
    LeafUpdate = 0,
    /// Tree structure changed (expiration, merge/split). Score = block's blue_score.
    Structural = 1,
}

/// Score index key.
///
/// Layout: `prefix(1) | rev_blue_score(8) | kind(1) | block_hash(32)` = 42 bytes.
/// Value: concatenated lane keys (`N * 32` bytes).
/// Uses [`ReverseBlueScore`] so forward iteration yields most recent touches first.
///
/// `kind` sorts before `block_hash`, so all `LeafUpdate` entries for a given score
/// are grouped together and sort before all `Structural` entries at that score.
/// This lets `get_leaf_updates` seek past the `Structural` group efficiently.
#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy)]
#[repr(C)]
pub struct ScoreIndexKey {
    pub prefix: u8,
    pub rev_blue_score: ReverseBlueScore,
    pub kind: ScoreIndexKind,
    pub block_hash: Hash,
}

impl ScoreIndexKey {
    pub fn new(prefix: u8, blue_score: u64, kind: ScoreIndexKind, block_hash: Hash) -> Self {
        Self { prefix, rev_blue_score: ReverseBlueScore::new(blue_score), kind, block_hash }
    }

    /// Build a seek key for LeafUpdate entries at or before `target_blue_score`.
    pub fn seek_key(prefix: u8, target_blue_score: u64) -> Self {
        Self {
            prefix,
            rev_blue_score: ReverseBlueScore::new(target_blue_score),
            kind: ScoreIndexKind::LeafUpdate,
            block_hash: Hash::from_bytes([0; 32]),
        }
    }

    /// Seek key for Structural entries at a specific score.
    pub fn seek_structural(prefix: u8, blue_score: u64) -> Self {
        Self {
            prefix,
            rev_blue_score: ReverseBlueScore::new(blue_score),
            kind: ScoreIndexKind::Structural,
            block_hash: Hash::from_bytes([0; 32]),
        }
    }

    /// Score prefix: `prefix(1)` = 1 byte.
    pub const SCORE_PREFIX_LEN: usize = 1;

    /// Parse a `ScoreIndexKey` from a byte slice that may have a trailing batch_id suffix.
    ///
    /// Keys are 42 bytes (normal) or 46 bytes (IBD with `u32` batch_id suffix).
    /// Returns a reference to the 42-byte prefix interpreted as `ScoreIndexKey`.
    pub fn try_ref_from_key_bytes(bytes: &[u8]) -> Result<&Self, zerocopy::error::TryCastError<&[u8], Self>> {
        let (key, _suffix) = Self::try_ref_from_prefix(bytes)?;
        Ok(key)
    }
}

/// Score index key with batch_id suffix for IBD.
///
/// Layout: `prefix(1) | rev_blue_score(8) | kind(1) | block_hash(32) | batch_id(4)` = 46 bytes.
/// The batch_id prevents key collisions when multiple IBD chunks produce entries
/// with the same `(blue_score, kind, block_hash)`.
#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy)]
#[repr(C)]
pub struct BatchedScoreIndexKey {
    pub base: ScoreIndexKey,
    pub batch_id: U32LE,
}

impl BatchedScoreIndexKey {
    pub fn new(prefix: u8, blue_score: u64, kind: ScoreIndexKind, block_hash: Hash, batch_id: u32) -> Self {
        Self { base: ScoreIndexKey::new(prefix, blue_score, kind, block_hash), batch_id: U32LE::new(batch_id) }
    }
}

impl AsRef<[u8]> for BatchedScoreIndexKey {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsRef<[u8]> for ScoreIndexKey {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zerocopy::IntoBytes;

    #[test]
    fn branch_version_key_layout() {
        let key = BranchVersionKey::new(0x47, 7, Hash::from_bytes([0x11; 32]), 1000, Hash::from_bytes([0x22; 32]));
        let bytes = key.as_bytes();
        assert_eq!(bytes.len(), 74);
        assert_eq!(bytes[0], 0x47); // prefix
        assert_eq!(bytes[1], 7); // depth
        assert_eq!(&bytes[2..34], &[0x11; 32]); // node_key
        let rev = u64::MAX - 1000;
        assert_eq!(&bytes[34..42], &rev.to_be_bytes());
        assert_eq!(&bytes[42..74], &[0x22; 32]); // block_hash
    }

    #[test]
    fn score_index_key_layout() {
        let key = ScoreIndexKey::new(0x4A, 500, ScoreIndexKind::Structural, Hash::from_bytes([0x44; 32]));
        let bytes = key.as_bytes();
        assert_eq!(bytes.len(), 42);
        assert_eq!(bytes[0], 0x4A);
        let rev = u64::MAX - 500;
        assert_eq!(&bytes[1..9], &rev.to_be_bytes());
        assert_eq!(bytes[9], 1); // kind = Structural
        assert_eq!(&bytes[10..42], &[0x44; 32]);
    }

    #[test]
    fn leaf_update_sorts_before_structural() {
        let leaf = ScoreIndexKey::new(0x4A, 500, ScoreIndexKind::LeafUpdate, Hash::from_bytes([0xFF; 32]));
        let structural = ScoreIndexKey::new(0x4A, 500, ScoreIndexKind::Structural, Hash::from_bytes([0x00; 32]));
        assert!(leaf.as_bytes() < structural.as_bytes(), "LeafUpdate (kind=0) must sort before Structural (kind=1) at same score");
    }
}
