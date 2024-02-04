pub mod core;
mod stores;
pub use core::*;
mod index;

pub const IDENT: &str = "ScoreIndex";

//Expose all things intended for external usage.
pub use crate::index::ScoreIndex; //we expose this separately to initiate the index.
