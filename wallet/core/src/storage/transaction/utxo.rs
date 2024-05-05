//!
//! UTXO record representation used by wallet transactions.
//!

use crate::imports::*;
use kaspa_addresses::Address;
use serde::{Deserialize, Serialize};

pub use kaspa_consensus_core::tx::TransactionId;

/// [`UtxoRecord`] represents an incoming transaction UTXO entry
/// stored within [`TransactionRecord`].
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct UtxoRecord {
    pub address: Option<Address>,
    pub index: TransactionIndexType,
    pub amount: u64,
    #[serde(rename = "scriptPubKey")]
    pub script_public_key: ScriptPublicKey,
    #[serde(rename = "isCoinbase")]
    pub is_coinbase: bool,
}

impl From<&UtxoEntryReference> for UtxoRecord {
    fn from(utxo: &UtxoEntryReference) -> Self {
        let UtxoEntryReference { utxo } = utxo;
        UtxoRecord {
            index: utxo.outpoint.get_index(),
            address: utxo.address.clone(),
            amount: utxo.amount,
            script_public_key: utxo.script_public_key.clone(),
            is_coinbase: utxo.is_coinbase,
        }
    }
}
