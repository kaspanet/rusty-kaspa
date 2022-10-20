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

/// Integer type for accumulated PoW of blue blocks. We expect no more than
/// 2^128 work in a single block (btc has ~2^80), and no more than 2^64
/// overall blocks, so 2^192 is definitely a justified upper-bound.  
pub type BlueWorkType = math::Uint192;

/// Defined in order to make it easy to experiment with various hashers
pub type BlockHashSet = HashSet<Hash>;

/// Defined in order to make it easy to experiment with various hashers
pub type BlockHashMap<T> = HashMap<Hash, T>;
