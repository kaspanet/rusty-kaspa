//! Pre-Toccata shadow types for `UtxoDiff` / `UtxoEntry`, used by the DB
//! access layer when it encounters a row written by a pre-Toccata node.
//!
//! The post-Toccata `UtxoEntry` gained a trailing `covenant_id:
//! Option<Hash>` field (see `consensus/core/src/tx/utxo_entry.rs`). Its
//! standalone bincode decoder can tolerate the missing tag byte via an
//! "EOF => None" trick, but that trick does not compose inside a
//! container — once the first entry is decoded the reader sits on the
//! next entry's bytes, not at EOF, and the trailing-Option probe reads
//! into the following entry instead. `UtxoDiff` stores `UtxoEntry`
//! values inside two `HashMap`s, so direct bincode deserialization of a
//! pre-Toccata `UtxoDiff` blob fails with `Io(UnexpectedEof)`.
//!
//! Rather than patching `UtxoEntry`'s serde, we keep the live types
//! unchanged and let the DB access layer ([`CachedDbAccess`] in
//! `kaspa-database`) dispatch to these shadow types when it reads a
//! row whose key is missing the post-Toccata version suffix.
//! [`PreToccataUtxoEntry`] derives a plain four-field `Deserialize` that
//! composes cleanly inside `HashMap`, and [`PreToccataUtxoDiff`] uses it
//! for both the `add` and `remove` collections.
//!
//! `From<PreToccataUtxoDiff> for Arc<UtxoDiff>` performs the on-read
//! conversion, defaulting every entry's `covenant_id` to `None`.
//!
//! Visibility is `pub` at the crate root so `DbUtxoDiffsStore` in the
//! `kaspa-consensus` crate can name it as the `TLegacy` type parameter
//! on `CachedDbAccess`.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
use crate::utxo::utxo_diff::UtxoDiff;

/// Pre-Toccata layout of [`UtxoEntry`]: four fields and no trailing
/// `covenant_id` tag. Deserializing this type through a derived
/// `Deserialize` consumes exactly the pre-Toccata byte sequence and
/// composes inside container types such as `HashMap`.
#[derive(Debug, Clone, Deserialize)]
pub struct PreToccataUtxoEntry {
    pub amount: u64,
    pub script_public_key: ScriptPublicKey,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

/// Pre-Toccata layout of [`UtxoDiff`]. Structurally identical to the live
/// type except every nested entry is a [`PreToccataUtxoEntry`].
#[derive(Debug, Clone, Deserialize)]
pub struct PreToccataUtxoDiff {
    pub add: HashMap<TransactionOutpoint, PreToccataUtxoEntry>,
    pub remove: HashMap<TransactionOutpoint, PreToccataUtxoEntry>,
}

impl From<PreToccataUtxoEntry> for UtxoEntry {
    fn from(e: PreToccataUtxoEntry) -> Self {
        UtxoEntry {
            amount: e.amount,
            script_public_key: e.script_public_key,
            block_daa_score: e.block_daa_score,
            is_coinbase: e.is_coinbase,
            covenant_id: None,
        }
    }
}

impl From<PreToccataUtxoDiff> for UtxoDiff {
    fn from(d: PreToccataUtxoDiff) -> Self {
        UtxoDiff {
            add: d.add.into_iter().map(|(k, v)| (k, v.into())).collect(),
            remove: d.remove.into_iter().map(|(k, v)| (k, v.into())).collect(),
        }
    }
}

impl From<PreToccataUtxoDiff> for Arc<UtxoDiff> {
    fn from(d: PreToccataUtxoDiff) -> Self {
        Arc::new(UtxoDiff::from(d))
    }
}
