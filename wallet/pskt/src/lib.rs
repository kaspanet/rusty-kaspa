//!
//! PSKT is a crate for working with Partially Signed Kaspa Transactions (PSKTs).
//! This crate provides following primitives: `PSKT`, `PSKTBuilder` and `Bundle`.
//! The `Bundle` struct is used for PSKT exchange payload serialization and carries
//! multiple `PSKT` instances allowing for exchange of Kaspa sweep transactions.
//!

pub mod bundle;
pub mod error;
pub mod global;
pub mod input;
pub mod output;
pub mod pskt;
pub mod role;
pub mod wasm;

mod convert;
mod utils;

pub mod prelude {
    pub use crate::bundle::Bundle;
    pub use crate::bundle::*;
    pub use crate::global::Global;
    pub use crate::input::Input;
    pub use crate::output::Output;
    pub use crate::pskt::*;

    // not quite sure why it warns of unused imports,
    // perhaps due to the fact that enums have no variants?
    #[allow(unused_imports)]
    pub use crate::role::*;
}
