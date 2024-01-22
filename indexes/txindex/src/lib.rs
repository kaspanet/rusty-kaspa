pub mod core; //all things visible to the outside
mod index;
mod stores;

pub use crate::core::*; //Expose all things intended for external usage.
pub use crate::index::TxIndex; //we expose this separately to initiate the index.

const IDENT: &str = "txindex";
