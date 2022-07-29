pub mod api;
pub mod services;
pub mod staging;
pub mod stores;

use api::hash::Hash;

/// `model::VIRTUAL` is a special hash representing the `virtual` block.
pub const VIRTUAL: Hash = Hash::VIRTUAL;

/// `model::ORIGIN` is a special hash representing a `virtual genesis` block.
/// It serves as a special local block which all locally-known
/// blocks are in its future.
pub const ORIGIN: Hash = Hash::ORIGIN;
