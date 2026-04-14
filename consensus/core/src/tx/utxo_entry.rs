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
        if deserializer.is_human_readable() {
            UtxoEntryHumanReadable::deserialize(deserializer).map(Into::into)
        } else {
            struct UtxoEntryVisitor;

            impl<'de> Visitor<'de> for UtxoEntryVisitor {
                type Value = UtxoEntry;

                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a UtxoEntry tuple (amount, script_public_key, block_daa_score, is_coinbase[, covenant_id])")
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

            deserializer.deserialize_tuple(5, UtxoEntryVisitor)
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
    const PRE_TOCCATA_AMOUNT: u64 = 0x0123_4567_89ab_cdef;
    const PRE_TOCCATA_DAA_SCORE: u64 = 42;
    const PRE_TOCCATA_SPK_SCRIPT: &[u8] = &[0x76, 0xa9, 0x14, 0x01, 0x02, 0x03];

    fn pre_toccata_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; PRE_TOCCATA_HEX.len() / 2];
        faster_hex::hex_decode(PRE_TOCCATA_HEX.as_bytes(), &mut bytes).unwrap();
        bytes
    }

    fn spk(version: u16, script: &[u8]) -> ScriptPublicKey {
        ScriptPublicKey::new(version, script.iter().copied().collect())
    }

    #[test]
    fn decode_pre_toccata() {
        let decoded: UtxoEntry = bincode::deserialize(&pre_toccata_bytes()).expect("decode pre-Toccata UtxoEntry");

        assert_eq!(decoded.amount, PRE_TOCCATA_AMOUNT, "amount");
        assert_eq!(decoded.block_daa_score, PRE_TOCCATA_DAA_SCORE, "block_daa_score");
        assert!(decoded.is_coinbase, "is_coinbase");
        assert_eq!(decoded.script_public_key.version, 0, "spk version");
        assert_eq!(decoded.script_public_key.script(), PRE_TOCCATA_SPK_SCRIPT, "spk script");
        assert_eq!(decoded.covenant_id, None, "covenant_id");
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
