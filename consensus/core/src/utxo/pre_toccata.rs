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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct PreToccataUtxoEntry {
    pub amount: u64,
    pub script_public_key: ScriptPublicKey,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

/// Pre-Toccata layout of [`UtxoDiff`]. Structurally identical to the live
/// type except every nested entry is a [`PreToccataUtxoEntry`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
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

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::Hash;
    use smallvec::SmallVec;

    fn sample_pre_entry(amount: u64, daa: u64, coinbase: bool) -> PreToccataUtxoEntry {
        PreToccataUtxoEntry {
            amount,
            script_public_key: ScriptPublicKey::new(0, SmallVec::from_slice(&[0x76, 0xa9, 0x14, 0x01, 0x02, 0x03])),
            block_daa_score: daa,
            is_coinbase: coinbase,
        }
    }

    fn outpoint(byte: u8, index: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash::from_bytes([byte; 32]), index)
    }

    #[test]
    fn pre_toccata_entry_self_roundtrip() {
        let entry = sample_pre_entry(0x0123_4567_89ab_cdef, 42, true);
        let bytes = bincode::serialize(&entry).unwrap();
        let decoded: PreToccataUtxoEntry = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, entry);
    }

    #[test]
    fn pre_toccata_entry_decodes_as_post_toccata() {
        let entry = sample_pre_entry(1_000, 7, false);
        let bytes = bincode::serialize(&entry).unwrap();
        let decoded: UtxoEntry = bincode::deserialize(&bytes).expect("post-Toccata decoder reads pre-Toccata bytes");
        let expected = UtxoEntry::new(entry.amount, entry.script_public_key.clone(), entry.block_daa_score, entry.is_coinbase, None);
        assert_eq!(decoded, expected);
    }

    /// Pre-Toccata `UtxoDiff` bytes cannot be decoded directly through post-Toccata
    /// `UtxoDiff`: the EOF-tolerance trick on `UtxoEntry` does not compose inside a
    /// `HashMap`, so the read fails. The DB access layer dispatches through
    /// `PreToccataUtxoDiff` and converts via `From`. Test mirrors that path.
    #[test]
    fn pre_toccata_diff_via_shadow_yields_post_toccata() {
        let mut diff = PreToccataUtxoDiff { add: HashMap::new(), remove: HashMap::new() };
        diff.add.insert(outpoint(0xaa, 0), sample_pre_entry(100, 1, false));
        diff.add.insert(outpoint(0xbb, 1), sample_pre_entry(200, 2, true));
        diff.remove.insert(outpoint(0xcc, 2), sample_pre_entry(300, 3, false));

        let bytes = bincode::serialize(&diff).unwrap();

        // Direct decode through `UtxoDiff` must fail — that's the failure mode that motivated the shadow type.
        assert!(bincode::deserialize::<UtxoDiff>(&bytes).is_err());

        // The supported path: decode via the shadow and convert.
        let shadow: PreToccataUtxoDiff = bincode::deserialize(&bytes).expect("decode pre-Toccata UtxoDiff via shadow");
        let converted: UtxoDiff = shadow.into();

        let expected: UtxoDiff = diff.into();
        assert_eq!(converted, expected);
    }
}
