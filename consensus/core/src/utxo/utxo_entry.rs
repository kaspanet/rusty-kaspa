use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::de::{Error as DeError, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use wasm_bindgen::prelude::*;

use crate::tx::ScriptPublicKey;

/// Holds details about an individual transaction output in a utxo
/// set such as whether or not it was contained in a coinbase tx, the daa
/// score of the block that accepts the tx, its public key script, and how
/// much it pays.
/// @category Consensus
#[derive(Debug, Default, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize)]
#[wasm_bindgen(inspectable, js_name = TransactionUtxoEntry)]
#[serde(rename_all = "camelCase")]
pub struct UtxoEntry {
    pub amount: u64,
    #[wasm_bindgen(js_name = scriptPublicKey, getter_with_clone)]
    pub script_public_key: ScriptPublicKey,
    #[wasm_bindgen(js_name = blockDaaScore)]
    pub block_daa_score: u64,
    #[wasm_bindgen(js_name = isCoinbase)]
    pub is_coinbase: bool,
    #[wasm_bindgen(js_name = covenantId)]
    #[serde(default)]
    pub covenant_id: Option<Hash>,
}

impl UtxoEntry {
    pub fn new(
        amount: u64,
        script_public_key: ScriptPublicKey,
        block_daa_score: u64,
        is_coinbase: bool,
        covenant_id: Option<Hash>,
    ) -> Self {
        Self { amount, script_public_key, block_daa_score, is_coinbase, covenant_id }
    }
}

impl MemSizeEstimator for UtxoEntry {}

/// Shadow struct used on the human-readable serde path so JSON/RPC consumers
/// keep seeing a camelCase object shape with `covenant_id` optional.
///
/// Human-readable formats (JSON, YAML, TOML) retain field labels on the wire,
/// so an absent `covenant_id` is naturally handled by `#[serde(default)]`.
/// The EOF-tolerance trick the bincode path uses below is unnecessary here.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UtxoEntryHumanReadable {
    amount: u64,
    script_public_key: ScriptPublicKey,
    block_daa_score: u64,
    is_coinbase: bool,
    #[serde(default)]
    covenant_id: Option<Hash>,
}

impl From<UtxoEntryHumanReadable> for UtxoEntry {
    fn from(e: UtxoEntryHumanReadable) -> Self {
        Self {
            amount: e.amount,
            script_public_key: e.script_public_key,
            block_daa_score: e.block_daa_score,
            is_coinbase: e.is_coinbase,
            covenant_id: e.covenant_id,
        }
    }
}

impl<'de> Deserialize<'de> for UtxoEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UtxoEntryVisitor;

        impl<'de> Visitor<'de> for UtxoEntryVisitor {
            type Value = UtxoEntry;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("struct UtxoEntry")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<UtxoEntry, A::Error> {
                let amount: u64 = seq.next_element()?.ok_or_else(|| DeError::invalid_length(0, &self))?;
                let script_public_key: ScriptPublicKey = seq.next_element()?.ok_or_else(|| DeError::invalid_length(1, &self))?;
                let block_daa_score: u64 = seq.next_element()?.ok_or_else(|| DeError::invalid_length(2, &self))?;
                let is_coinbase: bool = seq.next_element()?.ok_or_else(|| DeError::invalid_length(3, &self))?;
                // Pre-Toccata entries have no trailing Option tag; the
                // bincode reader hits EOF here and we treat that as None.
                let covenant_id: Option<Hash> = match seq.next_element::<Option<Hash>>() {
                    Ok(Some(value)) => value,
                    Ok(None) | Err(_) => None,
                };

                Ok(UtxoEntry { amount, script_public_key, block_daa_score, is_coinbase, covenant_id })
            }
        }

        if deserializer.is_human_readable() {
            UtxoEntryHumanReadable::deserialize(deserializer).map(Into::into)
        } else {
            const FIELDS: &[&str] = &["amount", "scriptPublicKey", "blockDaaScore", "isCoinbase", "covenantId"];
            deserializer.deserialize_struct("UtxoEntry", FIELDS, UtxoEntryVisitor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::ScriptPublicKey;

    /// Bincode encoding produced by the pre-Toccata `UtxoEntry` (4-field layout,
    /// no trailing `covenant_id` Option tag).
    const PRE_TOCCATA_HEX: &str = "efcdab89674523010000060000000000000076a9140102032a0000000000000001";
    /// Post-Toccata wire for the same logical entry with `covenant_id = None`
    /// (4 fields + Option tag `0x00`).
    const POST_TOCCATA_NONE_HEX: &str = "efcdab89674523010000060000000000000076a9140102032a000000000000000100";
    /// Post-Toccata wire for the same logical entry with `covenant_id = Some(Hash::from_bytes([0x5a; 32]))`
    /// (4 fields + Option tag `0x01` + 32-byte hash).
    const POST_TOCCATA_SOME_HEX: &str = "efcdab89674523010000060000000000000076a9140102032a000000000000000101\
                                         5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a";
    const SHARED_AMOUNT: u64 = 0x0123_4567_89ab_cdef;
    const SHARED_DAA_SCORE: u64 = 42;
    const SHARED_SPK: &[u8] = &[0x76, 0xa9, 0x14, 0x01, 0x02, 0x03];

    fn bytes_from_hex(hex: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; hex.len() / 2];
        faster_hex::hex_decode(hex.as_bytes(), &mut bytes).unwrap();
        bytes
    }

    fn pre_toccata_bytes() -> Vec<u8> {
        bytes_from_hex(PRE_TOCCATA_HEX)
    }

    fn spk(version: u16, script: &[u8]) -> ScriptPublicKey {
        ScriptPublicKey::new(version, script.iter().copied().collect())
    }

    fn shared_expected(covenant_id: Option<Hash>) -> UtxoEntry {
        UtxoEntry::new(SHARED_AMOUNT, spk(0, SHARED_SPK), SHARED_DAA_SCORE, true, covenant_id)
    }

    #[test]
    fn decode_pre_toccata() {
        let decoded: UtxoEntry = bincode::deserialize(&pre_toccata_bytes()).expect("decode pre-Toccata UtxoEntry");
        assert_eq!(decoded, shared_expected(None));
    }

    /// Cross-check that the hex-pinned `PRE_TOCCATA_HEX` constant is genuinely the
    /// pre-Toccata wire format — independent of the EOF-tolerance trick in
    /// `UtxoEntry::deserialize`. Decodes the same bytes through
    /// `PreToccataUtxoEntry`'s plain four-field `Deserialize` and re-encodes
    /// to confirm both directions match.
    #[test]
    fn pre_toccata_hex_decodes_as_pre_toccata_entry() {
        use crate::utxo::pre_toccata::PreToccataUtxoEntry;
        let bytes = pre_toccata_bytes();
        let decoded: PreToccataUtxoEntry = bincode::deserialize(&bytes).unwrap();
        let expected = PreToccataUtxoEntry {
            amount: SHARED_AMOUNT,
            script_public_key: spk(0, SHARED_SPK),
            block_daa_score: SHARED_DAA_SCORE,
            is_coinbase: true,
        };
        assert_eq!(decoded, expected);
        assert_eq!(bincode::serialize(&expected).unwrap(), bytes, "encode must reproduce frozen wire");
    }

    #[test]
    fn decode_post_toccata_none() {
        let bytes = bytes_from_hex(POST_TOCCATA_NONE_HEX);
        let decoded: UtxoEntry = bincode::deserialize(&bytes).expect("decode post-Toccata None UtxoEntry");
        let expected = shared_expected(None);
        assert_eq!(decoded, expected);
        assert_eq!(bincode::serialize(&expected).unwrap(), bytes, "encode must reproduce frozen wire");
    }

    #[test]
    fn decode_post_toccata_some() {
        let bytes = bytes_from_hex(POST_TOCCATA_SOME_HEX);
        let decoded: UtxoEntry = bincode::deserialize(&bytes).expect("decode post-Toccata Some UtxoEntry");
        let expected = shared_expected(Some(Hash::from_bytes([0x5a; 32])));
        assert_eq!(decoded, expected);
        assert_eq!(bincode::serialize(&expected).unwrap(), bytes, "encode must reproduce frozen wire");
    }

    #[test]
    fn bincode_roundtrip_pre_toccata() {
        let decoded: UtxoEntry = bincode::deserialize(&pre_toccata_bytes()).expect("decode pre-Toccata UtxoEntry");
        let re_encoded = bincode::serialize(&decoded).unwrap();
        let redecoded: UtxoEntry = bincode::deserialize(&re_encoded).expect("re-decode UtxoEntry");
        assert_eq!(decoded, redecoded);
    }

    #[test]
    fn bincode_roundtrip_post_toccata() {
        let utxo = UtxoEntry::new(1_000, spk(0, &[0x01, 0x02]), 777, false, Some(Hash::from_bytes([0x5a; 32])));
        let bytes = bincode::serialize(&utxo).unwrap();
        let decoded: UtxoEntry = bincode::deserialize(&bytes).unwrap();
        assert_eq!(utxo, decoded);
    }

    #[test]
    fn json_roundtrip_without_covenant() {
        let utxo = UtxoEntry::new(42, spk(0, &[0x76, 0xa9, 0x14]), 7, true, None);
        let json = serde_json::to_string(&utxo).unwrap();
        let decoded: UtxoEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(utxo, decoded);
    }

    #[test]
    fn json_roundtrip_with_covenant() {
        let utxo = UtxoEntry::new(123_456, spk(0, &[0xab, 0xcd]), 99, false, Some(Hash::from_bytes([0x5a; 32])));
        let json = serde_json::to_string(&utxo).unwrap();
        let decoded: UtxoEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(utxo, decoded);
    }
}
