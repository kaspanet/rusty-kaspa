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
            amount: utxo.entry.amount,
            script_public_key: utxo.entry.script_public_key.clone(),
            is_coinbase: utxo.entry.is_coinbase,
        }
    }
}

impl TryFrom<JsValue> for UtxoRecord {
    type Error = Error;

    fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
        let object = Object::try_from(&value).ok_or_else(|| Error::Custom("value must be of type object".to_string()))?;
        let address = object.try_get_value("address")?.map(|jsv| Address::try_from(jsv)).transpose()?;
        let index = object.get_u32("index")?;
        let amount = object.get_u64("amount")?;
        let script_public_key = object.get_value("scriptPubKey")?.try_into()?;
        let is_coinbase = object.get_bool("isCoinbase")?;

        Ok(UtxoRecord { address, index, amount, script_public_key, is_coinbase })
    }
}
