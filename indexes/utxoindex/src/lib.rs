pub mod core; //all things visable to the outside
mod index;
mod stores;
pub mod update_container;
pub mod test_helpers;

pub use crate::core::*; //Expose all things intended for external usage.
pub use crate::index::UtxoIndex; //we expose this seperatly to intiate the index.

#[cfg(test)]
pub use crate::test_helpers::VirtualChangeEmulator; //exposed for testing

const IDENT: &str = "utxoindex";
