use crate::imports::*;
use crate::runtime::Wallet;
use crate::storage::Binding;
use crate::utxo::{UtxoContext, UtxoEntryReference};
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::ScriptPublicKey;
use serde::{Deserialize, Serialize};
use workflow_log::style;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Credit,
    Debit,
    Reorg,
}

impl TransactionType {
    pub fn style(&self, s: &str) -> String {
        match self {
            TransactionType::Credit => style(s).green().to_string(),
            TransactionType::Debit => style(s).red().to_string(),
            TransactionType::Reorg => style(s).blue().to_string(),
        }
    }
    pub fn style_with_sign(&self, s: &str) -> String {
        match self {
            TransactionType::Credit => style("+".to_string() + s).green().to_string(),
            TransactionType::Debit => style("-".to_string() + s).red().to_string(),
            TransactionType::Reorg => style("-".to_string() + s).red().to_string(),
        }
    }
}

impl TransactionType {
    pub fn sign(&self) -> String {
        match self {
            TransactionType::Credit => "+",
            TransactionType::Debit => "-",
            TransactionType::Reorg => "-",
        }
        .to_string()
    }
}

// impl ToString for TransactionType {
//     fn to_string(&self) -> String {
//         match self {
//             TransactionType::Credit => "credit",
//             TransactionType::Debit => "debit",
//             TransactionType::Reorg => "reorg",
//         }
//         .to_string()
//     }
// }

impl std::fmt::Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TransactionType::Credit => "credit",
            TransactionType::Debit => "debit",
            TransactionType::Reorg => "reorg",
        };
        write!(f, "{s}")
    }
}

// #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
// pub struct TransactionRecordId(pub(crate) [u8; 8]);

// impl TransactionRecordId {
//     pub(crate) fn new(utxo: &UtxoEntryReference) -> TransactionRecordId {
//         Self::new_from_slice(&sha256_hash(&utxo.id().to_bytes()).unwrap().as_ref()[0..8])
//     }
//     pub fn new_from_slice(vec: &[u8]) -> Self {
//         Self(<[u8; 8]>::try_from(<&[u8]>::clone(&vec)).expect("Error: invalid slice size for id"))
//     }
//     pub fn short(&self) -> String {
//         let hex = self.to_hex();
//         format!("{}..{}", &hex[0..4], &hex[hex.len() - 4..])
//     }
// }

// impl ToHex for TransactionRecordId {
//     fn to_hex(&self) -> String {
//         self.0.to_vec().to_hex()
//     }
// }

// impl Serialize for TransactionRecordId {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         serializer.serialize_str(&hex_string(&self.0))
//     }
// }

// impl<'de> Deserialize<'de> for TransactionRecordId {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         let s = <std::string::String as Deserialize>::deserialize(deserializer)?;
//         let mut data = vec![0u8; s.len() / 2];
//         hex_decode(s.as_bytes(), &mut data).map_err(serde::de::Error::custom)?;
//         Ok(Self::new_from_slice(&data))
//     }
// }

// impl std::fmt::Display for TransactionRecordId {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{}", self.to_hex())
//     }
// }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UtxoRecord {
    pub address: Option<Address>,
    pub index: TransactionIndexType,
    pub amount: u64,
    #[serde(rename = "scriptPubKey")]
    pub script_public_key: ScriptPublicKey,
    #[serde(rename = "isCoinbase")]
    pub is_coinbase: bool,
}

impl From<UtxoEntryReference> for UtxoRecord {
    fn from(utxo: UtxoEntryReference) -> Self {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionMetadata {
    pub id: TransactionId,
    pub metadata: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub id: TransactionId,
    pub unixtime: u64,
    pub binding: Binding,
    #[serde(rename = "blockDaaScore")]
    pub block_daa_score: u64,
    #[serde(rename = "type")]
    pub transaction_type: TransactionType,
    #[serde(rename = "network")]
    pub network_id: NetworkId,
    #[serde(rename = "utxoEntries")]
    pub utxo_entries: Vec<UtxoRecord>,
}

impl TransactionRecord {
    pub fn network_id(&self) -> &NetworkId {
        &self.network_id
    }

    pub fn binding(&self) -> &Binding {
        &self.binding
    }
}

impl TransactionRecord {
    pub fn format(&self, wallet: &Wallet) -> String {
        let TransactionRecord { id, binding, block_daa_score, transaction_type, utxo_entries, .. } = self;

        let name = match binding {
            Binding::Custom(id) => style(id.short()).cyan(),
            Binding::Account(account_id) => {
                if let Some(account) = wallet.account_with_id(account_id).ok().flatten() {
                    style(account.name_or_id()).cyan()
                } else {
                    style(account_id.short() + " ??").magenta()
                }
            }
        };

        let kind = transaction_type.style(&transaction_type.to_string().pad_to_width(8));

        let mut lines = vec![format!("{name} {id} @{block_daa_score} DAA - {kind}")];

        let suffix = utils::kaspa_suffix(&self.network_id.network_type);

        for utxo_entry in utxo_entries {
            let address =
                style(utxo_entry.address.as_ref().map(|addr| addr.to_string()).unwrap_or_else(|| "n/a".to_string())).yellow();
            let is_coinbase = if utxo_entry.is_coinbase { style("(coinbase tx)").dim() } else { style("(standard tx)").dim() };
            let index = utxo_entry.index;
            let amount = transaction_type.style_with_sign(utils::sompi_to_kaspa_string(utxo_entry.amount).pad_to_width(19).as_str());

            lines.push(format!("    {address}"));
            lines.push(format!("    {index} {amount} {suffix} {is_coinbase}"));
        }

        lines.join("\r\n")
    }
}

// impl Zeroize for TransactionRecord {
//     fn zeroize(&mut self) {
//         self.0.zeroize()
//     }
// }

impl From<(&UtxoContext, TransactionType, TransactionId, Vec<UtxoEntryReference>)> for TransactionRecord {
    fn from(
        (utxo_context, transaction_type, id, utxos): (&UtxoContext, TransactionType, TransactionId, Vec<UtxoEntryReference>),
    ) -> Self {
        // let id = TransactionRecordId::new(&utxo);
        let binding = Binding::from(utxo_context.binding());
        // let UtxoEntryReference { utxo } = utxo;
        let block_daa_score = utxos[0].utxo.entry.block_daa_score;
        let utxo_entries = utxos.into_iter().map(UtxoRecord::from).collect::<Vec<_>>();

        TransactionRecord {
            id,
            unixtime: 0,
            binding,
            utxo_entries,
            // address: utxo.address.clone(),
            // outpoint: utxo.outpoint.clone().into(),
            // amount: utxo.entry.amount,
            // script_public_key: utxo.entry.script_public_key.clone(),
            block_daa_score, //: utxo.entry.block_daa_score,
            // is_coinbase: utxo.entry.is_coinbase,
            transaction_type,
            network_id: utxo_context.processor.network_id().expect("network expected for transaction record generation"),
        }
    }
}
