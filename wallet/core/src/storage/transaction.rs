use crate::imports::*;
use crate::runtime::Wallet;
use crate::storage::Binding;
use crate::utxo::{UtxoContext, UtxoEntryReference};
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::ScriptPublicKey;
use separator::Separatable;
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
    pub fn style_with_sign(&self, s: &str, history: bool) -> String {
        match self {
            TransactionType::Credit => style("+".to_string() + s).green().to_string(),
            TransactionType::Debit => style("-".to_string() + s).red().to_string(),
            TransactionType::Reorg => {
                if history {
                    style("".to_string() + s).dim()
                } else {
                    style("-".to_string() + s).red()
                }
            }
            .to_string(),
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
pub enum TransactionMetadata {
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    id: TransactionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    unixtime: Option<u64>,
    binding: Binding,
    #[serde(rename = "blockDaaScore")]
    block_daa_score: u64,
    #[serde(rename = "type")]
    transaction_type: TransactionType,
    #[serde(rename = "network")]
    network_id: NetworkId,
    #[serde(rename = "utxoEntries")]
    utxo_entries: Vec<UtxoRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<TransactionMetadata>,
}

impl TransactionRecord {
    pub fn id(&self) -> &TransactionId {
        &self.id
    }

    pub fn unixtime(&self) -> Option<u64> {
        self.unixtime
    }

    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    pub fn block_daa_score(&self) -> u64 {
        self.block_daa_score
    }

    pub fn transaction_type(&self) -> &TransactionType {
        &self.transaction_type
    }

    pub fn network_id(&self) -> &NetworkId {
        &self.network_id
    }

    pub fn is_coinbase(&self) -> bool {
        self.utxo_entries.iter().any(|entry| entry.is_coinbase)
    }
}

impl TransactionRecord {
    pub async fn format(&self, wallet: &Arc<Wallet>) -> String {
        self.format_with_args(wallet, None, None, false, None).await
    }

    pub async fn format_with_state(&self, wallet: &Arc<Wallet>, state: Option<&str>) -> String {
        self.format_with_args(wallet, state, None, false, None).await
    }

    pub async fn format_with_args(
        &self,
        wallet: &Arc<Wallet>,
        state: Option<&str>,
        current_daa_score: Option<u64>,
        history: bool,
        account: Option<Arc<runtime::Account>>,
    ) -> String {
        let TransactionRecord { id, binding, block_daa_score, transaction_type, utxo_entries, .. } = self;

        let name = match binding {
            Binding::Custom(id) => style(id.short()).cyan(),
            Binding::Account(account_id) => {
                let account = if let Some(account) = account {
                    Some(account)
                } else {
                    wallet.get_account_by_id(account_id).await.ok().flatten()
                };

                if let Some(account) = account {
                    style(account.name_or_id()).cyan()
                } else {
                    style(account_id.short() + " ??").magenta()
                }
            }
        };

        let kind = transaction_type.style(&transaction_type.to_string());

        let maturity = current_daa_score
            .map(|score| {
                // TODO - refactor @ high BPS processing
                let maturity = if self.is_coinbase() {
                    crate::utxo::entry::MATURITY_PERIOD_COINBASE_TRANSACTION
                } else {
                    crate::utxo::entry::MATURITY_PERIOD_USER_TRANSACTION
                };

                if score < self.block_daa_score() + maturity {
                    style("pending").dim().to_string()
                } else {
                    style("confirmed").dim().to_string()
                }
            })
            .unwrap_or_default();

        // let current_daa_score = current_daa_score.map(|score| score ).unwrap_or_else(|| "".to_string()
        let block_daa_score = block_daa_score.separated_string();
        let state = state.unwrap_or(&maturity);
        let mut lines = vec![format!("{name} {id} @{block_daa_score} DAA - {kind} {state}")];

        let suffix = utils::kaspa_suffix(&self.network_id.network_type);

        for utxo_entry in utxo_entries {
            let address = style(utxo_entry.address.as_ref().map(|addr| addr.to_string()).unwrap_or_else(|| "n/a".to_string())).blue();
            let index = utxo_entry.index;
            let is_coinbase = if utxo_entry.is_coinbase {
                style(format!("coinbase utxo [{index}]")).dim()
            } else {
                style(format!("standard utxo [{index}]")).dim()
            };
            let amount = transaction_type.style_with_sign(utils::sompi_to_kaspa_string(utxo_entry.amount).as_str(), history);

            lines.push(format!("    {address}"));
            lines.push(format!("    {amount} {suffix} {is_coinbase}"));
        }

        lines.join("\r\n")
    }
}

impl From<(&UtxoContext, TransactionType, TransactionId, Vec<UtxoEntryReference>)> for TransactionRecord {
    fn from(
        (utxo_context, transaction_type, id, utxos): (&UtxoContext, TransactionType, TransactionId, Vec<UtxoEntryReference>),
    ) -> Self {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score = utxos[0].utxo.entry.block_daa_score;
        let utxo_entries = utxos.into_iter().map(UtxoRecord::from).collect::<Vec<_>>();

        TransactionRecord {
            id,
            unixtime: None,
            binding,
            utxo_entries,
            block_daa_score,
            transaction_type,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
        }
    }
}
