use crate::encryption::sha256_hash;
use crate::imports::*;
use crate::runtime::{Account, AccountId, Wallet};
use crate::utxo::UtxoEntryReference;
use faster_hex::{hex_decode, hex_string};
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_utils::hex::ToHex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use workflow_log::style;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionType {
    Credit,
    Debit,
}

impl TransactionType {
    pub fn style(&self, s: &str) -> String {
        match self {
            TransactionType::Credit => style(s).green().to_string(),
            TransactionType::Debit => style(s).red().to_string(),
        }
    }
    pub fn style_with_sign(&self, s: &str) -> String {
        match self {
            TransactionType::Credit => style("+".to_string() + s).green().to_string(),
            TransactionType::Debit => style("-".to_string() + s).red().to_string(),
        }
    }
}

impl TransactionType {
    pub fn sign(&self) -> String {
        match self {
            TransactionType::Credit => "+",
            TransactionType::Debit => "-",
        }
        .to_string()
    }
}

impl ToString for TransactionType {
    fn to_string(&self) -> String {
        match self {
            TransactionType::Credit => "credit",
            TransactionType::Debit => "debit",
        }
        .to_string()
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionRecordId(pub(crate) [u8; 8]);

impl TransactionRecordId {
    pub(crate) fn new(utxo: &UtxoEntryReference) -> TransactionRecordId {
        Self::new_from_slice(&sha256_hash(&utxo.id().to_bytes()).unwrap().as_ref()[0..8])
    }
    pub fn new_from_slice(vec: &[u8]) -> Self {
        Self(<[u8; 8]>::try_from(<&[u8]>::clone(&vec)).expect("Error: invalid slice size for id"))
    }
    pub fn short(&self) -> String {
        let hex = self.to_hex();
        format!("{}..{}", &hex[0..4], &hex[hex.len() - 4..])
    }
}

impl ToHex for TransactionRecordId {
    fn to_hex(&self) -> String {
        self.0.to_vec().to_hex()
    }
}

impl Serialize for TransactionRecordId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex_string(&self.0))
    }
}

impl<'de> Deserialize<'de> for TransactionRecordId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <std::string::String as Deserialize>::deserialize(deserializer)?;
        let mut data = vec![0u8; s.len() / 2];
        hex_decode(s.as_bytes(), &mut data).map_err(serde::de::Error::custom)?;
        Ok(Self::new_from_slice(&data))
    }
}

impl std::fmt::Display for TransactionRecordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub id: TransactionRecordId,
    #[serde(rename = "accountId")]
    pub account_id: AccountId,
    pub address: Option<Address>,
    pub outpoint: cctx::TransactionOutpoint,
    pub amount: u64,
    #[serde(rename = "scriptPubKey")]
    pub script_public_key: ScriptPublicKey,
    #[serde(rename = "blockDaaScore")]
    pub block_daa_score: u64,
    #[serde(rename = "isCoinbase")]
    pub is_coinbase: bool,
    #[serde(rename = "transactionType")]
    pub transaction_type: TransactionType,
    // TODO: support network type
    // #[serde(rename = "networkType")]
    // network_type: NetworkType,
}

impl TransactionRecord {
    pub fn format(&self, wallet: &Wallet) -> String {
        let TransactionRecord { id, account_id, address, amount, is_coinbase, transaction_type, .. } = self;

        let address = style(address.as_ref().map(|addr| addr.short(16)).unwrap_or_else(|| "n/a".to_string())).yellow();
        let is_coinbase = if *is_coinbase { style("(coinbase tx)").dim() } else { style("(standard tx)").dim() };
        let id = style(id.short()).cyan();

        let name = if let Some(account) = wallet.account_with_id(account_id).ok().flatten() {
            style(account.name_or_id()).cyan()
        } else {
            style(account_id.short() + " ??").magenta()
        };

        let suffix = utils::kaspa_suffix(&wallet.network().unwrap());
        let amount = transaction_type.style_with_sign(utils::sompi_to_kaspa_string(*amount).as_str());

        let kind = transaction_type.style(&transaction_type.to_string().pad_to_width(8));

        // v.pad_to_width(c2),

        format!("{kind} {id} {name}  {address}  {amount} {suffix} {is_coinbase}")
    }
}

// impl Zeroize for TransactionRecord {
//     fn zeroize(&mut self) {
//         self.0.zeroize()
//     }
// }

impl From<(TransactionType, &Arc<Account>, UtxoEntryReference)> for TransactionRecord {
    fn from((transaction_type, account, utxo): (TransactionType, &Arc<Account>, UtxoEntryReference)) -> Self {
        let id = TransactionRecordId::new(&utxo);
        let UtxoEntryReference { utxo } = utxo;

        TransactionRecord {
            id,
            account_id: *account.id(),
            address: utxo.address.clone(),
            outpoint: utxo.outpoint.clone().into(),
            amount: utxo.entry.amount,
            script_public_key: utxo.entry.script_public_key.clone(),
            block_daa_score: utxo.entry.block_daa_score,
            is_coinbase: utxo.entry.is_coinbase,
            transaction_type,
        }
    }
}
