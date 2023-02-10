pub mod core; //all things visable to the outside
mod index;
mod stores;
mod update_container;

pub use crate::core::*; //Expose all things intended for external usage.
pub use crate::index::UtxoIndex; //we expose this separatly to intiate the index.

const IDENT: &str = "utxoindex";
