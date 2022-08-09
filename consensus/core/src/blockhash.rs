use hashes::{Hash, HASH_SIZE};
use std::sync::Arc;

pub type BlockHashes = Arc<Vec<Hash>>;

/// `blockhash::NONE` is a hash which is used in rare cases as the `None` block hash
pub const NONE: Hash = Hash::from_bytes([0u8; HASH_SIZE]);

/// `blockhash::VIRTUAL` is a special hash representing the `virtual` block.
pub const VIRTUAL: Hash = Hash::from_bytes([0xff; HASH_SIZE]);

/// `blockhash::ORIGIN` is a special hash representing a `virtual genesis` block.
/// It serves as a special local block which all locally-known
/// blocks are in its future.
pub const ORIGIN: Hash = Hash::from_bytes([0xfe; HASH_SIZE]);

pub trait BlockHashExtensions {
    fn is_none(&self) -> bool;
    fn is_virtual(&self) -> bool;
    fn is_origin(&self) -> bool;
}

impl BlockHashExtensions for Hash {
    fn is_none(&self) -> bool {
        self.eq(&NONE)
    }

    fn is_virtual(&self) -> bool {
        self.eq(&VIRTUAL)
    }

    fn is_origin(&self) -> bool {
        self.eq(&ORIGIN)
    }
}
