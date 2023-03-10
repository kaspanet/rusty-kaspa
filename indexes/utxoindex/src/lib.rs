pub mod core; //all things visible to the outside
mod index;
mod stores;
mod update_container;

#[cfg(test)]
mod testutils;

pub use crate::core::*; //Expose all things intended for external usage.
pub use crate::index::UtxoIndex; //we expose this separately to initiate the index.

const IDENT: &str = "utxoindex";
