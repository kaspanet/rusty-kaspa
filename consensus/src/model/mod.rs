pub mod api;
pub mod services;
pub mod staging;
pub mod stores;

use api::hash::Hash;

/// model::VIRTUAL represents a special hash representing the `virtual` block.
pub const VIRTUAL: Hash = Hash::VIRTUAL;

/// model::ORIGIN represent a special hash used as a virtual genesis.
/// It acts as a special local block which all locally-known
/// blocks are in its future.
pub const ORIGIN: Hash = Hash::ORIGIN;
