//! Support for BIP39 mnemonic phrases.
//!
//! Adapted from the `bip39` crate.
//! Copyright © 2017-2018 Stephen Oliver with contributions by Maciej Hirsz.

mod bits;
mod language;
mod phrase;

//#[cfg(feature = "bip39")]
pub(crate) mod seed;

pub use self::{language::Language, phrase::Mnemonic, phrase::WordCount};
