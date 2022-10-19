use std::collections::{HashMap, HashSet};

use hashes::Hash;

pub mod block;
pub mod blockhash;
pub mod hashing;
pub mod header;
pub mod merkle;
pub mod muhash;
pub mod subnets;
pub mod tx;
pub mod utxo;

// TODO: switch to math::Uint192
pub type BlueWorkType = u128;

/// Defined in order to make it easy to experiment with various hashers
pub type BlockHashSet = HashSet<Hash>;

/// Defined in order to make it easy to experiment with various hashers
pub type BlockHashMap<T> = HashMap<Hash, T>;
