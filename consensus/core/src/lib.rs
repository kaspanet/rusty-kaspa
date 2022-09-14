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

pub type BlueWorkType = u128;

/// Defined in order to make it easy to experiment with various hashers
pub type DomainHashSet = HashSet<Hash>;

/// Defined in order to make it easy to experiment with various hashers
pub type DomainHashMap<T> = HashMap<Hash, T>;
