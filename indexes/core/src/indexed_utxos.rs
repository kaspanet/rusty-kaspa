use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::de::{Error as DeError, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt;

// TODO: explore potential optimization via custom TransactionOutpoint hasher for below,
// One possible implementation: u64 of transaction id xor'd with 4 bytes of transaction index.
pub type CompactUtxoCollection = HashMap<TransactionOutpoint, CompactUtxoEntry>;

/// A collection of utxos indexed via; [`ScriptPublicKey`] => [`TransactionOutpoint`] => [`CompactUtxoEntry`].
pub type UtxoSetByScriptPublicKey = HashMap<ScriptPublicKey, CompactUtxoCollection>;

/// A map of balance by script public key
pub type BalanceByScriptPublicKey = HashMap<ScriptPublicKey, u64>;

// Note: memory optimization compared to go-lang kaspad:
// Unlike `consensus_core::tx::UtxoEntry` the utxoindex utilizes a compacted utxo form, where `script_public_key` field is removed.
// This utxo structure can be utilized in the utxoindex, since utxos are implicitly key'd via its script public key (and outpoint) at all times.
/// A compacted form of [`UtxoEntry`] without reference to [`ScriptPublicKey`] or [`TransactionOutpoint`]
#[derive(Clone, Copy, Serialize, Debug, PartialEq, Eq)]
pub struct CompactUtxoEntry {
    pub amount: u64,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
    pub covenant_id: Option<Hash>,
}

impl CompactUtxoEntry {
    /// Creates a new [`CompactUtxoEntry`]
    pub fn new(amount: u64, block_daa_score: u64, is_coinbase: bool, covenant_id: Option<Hash>) -> Self {
        Self { amount, block_daa_score, is_coinbase, covenant_id }
    }
}

impl MemSizeEstimator for CompactUtxoEntry {}

impl From<UtxoEntry> for CompactUtxoEntry {
    fn from(utxo_entry: UtxoEntry) -> Self {
        Self {
            amount: utxo_entry.amount,
            block_daa_score: utxo_entry.block_daa_score,
            is_coinbase: utxo_entry.is_coinbase,
            covenant_id: utxo_entry.covenant_id,
        }
    }
}

/// Shadow struct used on the human-readable serde path so JSON consumers keep
/// seeing the pre-Toccata object shape with `covenant_id` optional.
///
/// Human-readable formats (JSON, YAML, TOML) retain field labels on the wire,
/// so an absent `covenant_id` is naturally handled by `#[serde(default)]`.
/// The EOF-tolerance trick the bincode path uses below is unnecessary here.
#[derive(Deserialize)]
struct CompactUtxoEntryHumanReadable {
    amount: u64,
    block_daa_score: u64,
    is_coinbase: bool,
    #[serde(default)]
    covenant_id: Option<Hash>,
}

impl From<CompactUtxoEntryHumanReadable> for CompactUtxoEntry {
    fn from(e: CompactUtxoEntryHumanReadable) -> Self {
        Self { amount: e.amount, block_daa_score: e.block_daa_score, is_coinbase: e.is_coinbase, covenant_id: e.covenant_id }
    }
}

impl<'de> Deserialize<'de> for CompactUtxoEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            CompactUtxoEntryHumanReadable::deserialize(deserializer).map(Into::into)
        } else {
            struct CompactUtxoEntryVisitor;

            impl<'de> Visitor<'de> for CompactUtxoEntryVisitor {
                type Value = CompactUtxoEntry;

                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("struct CompactUtxoEntry")
                }

                fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<CompactUtxoEntry, A::Error> {
                    let amount: u64 = seq.next_element()?.ok_or_else(|| DeError::invalid_length(0, &self))?;
                    let block_daa_score: u64 = seq.next_element()?.ok_or_else(|| DeError::invalid_length(1, &self))?;
                    let is_coinbase: bool = seq.next_element()?.ok_or_else(|| DeError::invalid_length(2, &self))?;
                    // Pre-Toccata entries have no trailing Option tag; the
                    // bincode reader hits EOF here and we treat that as None.
                    let covenant_id: Option<Hash> = match seq.next_element::<Option<Hash>>() {
                        Ok(Some(value)) => value,
                        Ok(None) | Err(_) => None,
                    };
                    Ok(CompactUtxoEntry { amount, block_daa_score, is_coinbase, covenant_id })
                }
            }

            const FIELDS: &[&str] = &["amount", "blockDaaScore", "isCoinbase", "covenantId"];
            deserializer.deserialize_struct("CompactUtxoEntry", FIELDS, CompactUtxoEntryVisitor)
        }
    }
}

/// A struct holding utxo changes to the utxoindex via `added` and `removed` [`UtxoSetByScriptPublicKey`]'s
#[derive(Debug, Clone)]
pub struct UtxoChanges {
    pub added: UtxoSetByScriptPublicKey,
    pub removed: UtxoSetByScriptPublicKey,
}

impl UtxoChanges {
    /// Create a new [`UtxoChanges`] struct via supplied `added` and `removed` [`UtxoSetByScriptPublicKey`]'s
    pub fn new(added: UtxoSetByScriptPublicKey, removed: UtxoSetByScriptPublicKey) -> Self {
        Self { added, removed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bincode encoding produced by the pre-Toccata `CompactUtxoEntry` (3-field
    /// layout, no trailing `covenant_id` Option tag).
    const PRE_TOCCATA_HEX: &str = "efcdab89674523012a0000000000000001";
    /// Post-Toccata wire for the same logical entry with `covenant_id = None`.
    const POST_TOCCATA_NONE_HEX: &str = "efcdab89674523012a000000000000000100";
    /// Post-Toccata wire with `covenant_id = Some(Hash::from_bytes([0x5a; 32]))`.
    const POST_TOCCATA_SOME_HEX: &str =
        "efcdab89674523012a0000000000000001015a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a";
    const SHARED_AMOUNT: u64 = 0x0123_4567_89ab_cdef;
    const SHARED_DAA_SCORE: u64 = 42;

    fn bytes_from_hex(hex: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; hex.len() / 2];
        faster_hex::hex_decode(hex.as_bytes(), &mut bytes).unwrap();
        bytes
    }

    fn pre_toccata_bytes() -> Vec<u8> {
        bytes_from_hex(PRE_TOCCATA_HEX)
    }

    fn shared_expected(covenant_id: Option<Hash>) -> CompactUtxoEntry {
        CompactUtxoEntry::new(SHARED_AMOUNT, SHARED_DAA_SCORE, true, covenant_id)
    }

    #[test]
    fn decode_pre_toccata() {
        let decoded: CompactUtxoEntry = bincode::deserialize(&pre_toccata_bytes()).expect("decode pre-Toccata CompactUtxoEntry");
        assert_eq!(decoded, shared_expected(None));
    }

    #[test]
    fn decode_post_toccata_none() {
        let bytes = bytes_from_hex(POST_TOCCATA_NONE_HEX);
        let decoded: CompactUtxoEntry = bincode::deserialize(&bytes).expect("decode post-Toccata None CompactUtxoEntry");
        let expected = shared_expected(None);
        assert_eq!(decoded, expected);
        assert_eq!(bincode::serialize(&expected).unwrap(), bytes, "encode must reproduce frozen wire");
    }

    #[test]
    fn decode_post_toccata_some() {
        let bytes = bytes_from_hex(POST_TOCCATA_SOME_HEX);
        let decoded: CompactUtxoEntry = bincode::deserialize(&bytes).expect("decode post-Toccata Some CompactUtxoEntry");
        let expected = shared_expected(Some(Hash::from_bytes([0x5a; 32])));
        assert_eq!(decoded, expected);
        assert_eq!(bincode::serialize(&expected).unwrap(), bytes, "encode must reproduce frozen wire");
    }

    #[test]
    fn bincode_roundtrip_pre_toccata() {
        let decoded: CompactUtxoEntry = bincode::deserialize(&pre_toccata_bytes()).expect("decode pre-Toccata CompactUtxoEntry");
        let re_encoded = bincode::serialize(&decoded).unwrap();
        let redecoded: CompactUtxoEntry = bincode::deserialize(&re_encoded).expect("re-decode CompactUtxoEntry");
        assert_eq!(decoded, redecoded);
    }

    #[test]
    fn bincode_roundtrip_post_toccata() {
        let utxo = CompactUtxoEntry::new(1_000, 777, false, Some(Hash::from_bytes([0x5a; 32])));
        let bytes = bincode::serialize(&utxo).unwrap();
        let decoded: CompactUtxoEntry = bincode::deserialize(&bytes).unwrap();
        assert_eq!(utxo, decoded);
    }

    #[test]
    fn json_roundtrip_without_covenant() {
        let utxo = CompactUtxoEntry::new(42, 7, true, None);
        let json = serde_json::to_string(&utxo).unwrap();
        let decoded: CompactUtxoEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(utxo, decoded);
    }

    #[test]
    fn json_roundtrip_with_covenant() {
        let utxo = CompactUtxoEntry::new(123_456, 99, false, Some(Hash::from_bytes([0x5a; 32])));
        let json = serde_json::to_string(&utxo).unwrap();
        let decoded: CompactUtxoEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(utxo, decoded);
    }

    #[test]
    fn decode_pre_toccata_json() {
        // Legacy JSON payload emitted by pre-Toccata nodes (no `covenant_id` field).
        let legacy_json = r#"{"amount":42,"block_daa_score":7,"is_coinbase":true}"#;
        let decoded: CompactUtxoEntry = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(decoded.amount, 42);
        assert_eq!(decoded.block_daa_score, 7);
        assert!(decoded.is_coinbase);
        assert_eq!(decoded.covenant_id, None);
    }
}
