use crate::imports::*;
use crate::runtime::Wallet;
use crate::storage::Binding;
use crate::tx::{PendingTransaction, PendingTransactionInner};
use crate::utxo::{UtxoContext, UtxoEntryReference};
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint};
use separator::Separatable;
use serde::{Deserialize, Serialize};
use workflow_log::style;

const TRANSACTION_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    /// Incoming transaction
    Incoming,
    /// Transaction created by the runtime
    Outgoing,
    /// Outgoing transaction observed by the runtime
    External,
    /// Internal batch (sweep) transaction
    Batch,
    /// Reorg transaction (caused by UTXO reorg during mining)
    Reorg,
}

impl TransactionType {
    pub fn style(&self, s: &str) -> String {
        match self {
            TransactionType::Incoming => style(s).green().to_string(),
            TransactionType::Outgoing => style(s).red().to_string(),
            TransactionType::External => style(s).red().to_string(),
            TransactionType::Batch => style(s).blue().to_string(),
            TransactionType::Reorg => style(s).blue().to_string(),
        }
    }
    pub fn style_with_sign(&self, s: &str, history: bool) -> String {
        match self {
            TransactionType::Incoming => style("+".to_string() + s).green().to_string(),
            TransactionType::Outgoing => style("-".to_string() + s).red().to_string(),
            TransactionType::External => style("-".to_string() + s).red().to_string(),
            TransactionType::Batch => style("".to_string() + s).dim().to_string(),
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
            TransactionType::Incoming => "+",
            TransactionType::Outgoing => "-",
            TransactionType::External => "-",
            TransactionType::Batch => "",
            TransactionType::Reorg => "-",
        }
        .to_string()
    }
}

impl std::fmt::Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TransactionType::Incoming => "incoming",
            TransactionType::Outgoing => "outgoing",
            TransactionType::External => "external",
            TransactionType::Batch => "batch",
            TransactionType::Reorg => "reorg",
        };
        write!(f, "{s}")
    }
}

/// [`UtxoRecord`] represents an incoming transaction UTXO entry
/// stored within [`TransactionRecord`].
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
#[serde(tag = "type", content = "transaction")]
#[serde(rename_all = "lowercase")]
pub enum TransactionData {
    Reorg {
        #[serde(rename = "utxoEntries")]
        utxo_entries: Vec<UtxoRecord>,
        #[serde(rename = "value")]
        aggregate_input_value: u64,
    },
    Incoming {
        #[serde(rename = "utxoEntries")]
        utxo_entries: Vec<UtxoRecord>,
        #[serde(rename = "value")]
        aggregate_input_value: u64,
    },
    External {
        #[serde(rename = "utxoEntries")]
        utxo_entries: Vec<UtxoRecord>,
        #[serde(rename = "value")]
        aggregate_input_value: u64,
    },
    Outgoing {
        #[serde(rename = "isFinal")]
        is_final: bool,
        fees: u64,
        #[serde(rename = "inputValue")]
        aggregate_input_value: u64,
        #[serde(rename = "outputValue")]
        aggregate_output_value: u64,
        transaction: Transaction,
        #[serde(rename = "paymentValue")]
        payment_value: Option<u64>,
        #[serde(rename = "changeValue")]
        change_value: u64,
    },
}

impl TransactionData {
    pub fn transaction_type(&self) -> TransactionType {
        match self {
            TransactionData::Reorg { .. } => TransactionType::Reorg,
            TransactionData::Incoming { .. } => TransactionType::Incoming,
            TransactionData::External { .. } => TransactionType::External,
            TransactionData::Outgoing { is_final, .. } => {
                if *is_final {
                    TransactionType::Outgoing
                } else {
                    TransactionType::Batch
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    version: u16,
    id: TransactionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    unixtime: Option<u64>,
    binding: Binding,
    #[serde(rename = "blockDaaScore")]
    block_daa_score: u64,
    #[serde(rename = "network")]
    network_id: NetworkId,
    #[serde(rename = "data")]
    transaction_data: TransactionData,
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

    pub fn transaction_type(&self) -> TransactionType {
        self.transaction_data.transaction_type()
    }

    pub fn network_id(&self) -> &NetworkId {
        &self.network_id
    }

    pub fn is_coinbase(&self) -> bool {
        match &self.transaction_data {
            TransactionData::Incoming { utxo_entries, .. } => utxo_entries.iter().any(|entry| entry.is_coinbase),
            _ => false,
        }
    }
}

impl TransactionRecord {
    pub fn new_incoming(
        utxo_context: &UtxoContext,
        transaction_type: TransactionType,
        id: TransactionId,
        utxos: Vec<UtxoEntryReference>,
    ) -> Self {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score = utxos[0].utxo.entry.block_daa_score;
        let utxo_entries = utxos.into_iter().map(UtxoRecord::from).collect::<Vec<_>>();
        let aggregate_input_value = utxo_entries.iter().map(|utxo| utxo.amount).sum::<u64>();

        let transaction_data = match transaction_type {
            TransactionType::Incoming => TransactionData::Incoming { utxo_entries, aggregate_input_value },
            TransactionType::Reorg => TransactionData::Reorg { utxo_entries, aggregate_input_value },
            _ => panic!("TransactionRecord::new_incoming() - invalid transaction type"),
        };

        TransactionRecord {
            version: TRANSACTION_VERSION,
            id,
            unixtime: None,
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
        }
    }

    pub fn new_external(utxo_context: &UtxoContext, id: TransactionId, utxos: Vec<UtxoEntryReference>) -> Self {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score = utxos[0].utxo.entry.block_daa_score;
        let utxo_entries = utxos.into_iter().map(UtxoRecord::from).collect::<Vec<_>>();
        let aggregate_input_value = utxo_entries.iter().map(|utxo| utxo.amount).sum::<u64>();

        let transaction_data = TransactionData::External { utxo_entries, aggregate_input_value };

        TransactionRecord {
            version: TRANSACTION_VERSION,
            id,
            unixtime: None,
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
        }
    }

    pub fn new_outgoing(utxo_context: &UtxoContext, pending_tx: &PendingTransaction) -> Self {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score =
            utxo_context.processor().current_daa_score().expect("TransactionRecord::new_outgoing() - missing daa score");

        let PendingTransactionInner {
            signable_tx,
            kind,
            fees,
            aggregate_input_value,
            aggregate_output_value,
            payment_value,
            change_output_value,
            ..
        } = &*pending_tx.inner;

        let transaction = signable_tx.lock().unwrap().tx.clone();
        let id = transaction.id();

        let transaction_data = TransactionData::Outgoing {
            is_final: kind.is_final(),
            fees: *fees,
            aggregate_input_value: *aggregate_input_value,
            aggregate_output_value: *aggregate_output_value,
            transaction,
            payment_value: *payment_value,
            change_value: *change_output_value,
        };

        TransactionRecord {
            version: TRANSACTION_VERSION,
            id,
            unixtime: None,
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
        }
    }

    pub async fn format(&self, wallet: &Arc<Wallet>, include_utxos: bool) -> Vec<String> {
        self.format_with_args(wallet, None, None, include_utxos, false, None).await
    }

    pub async fn format_with_state(&self, wallet: &Arc<Wallet>, state: Option<&str>, include_utxos: bool) -> Vec<String> {
        self.format_with_args(wallet, state, None, include_utxos, false, None).await
    }

    pub async fn format_with_args(
        &self,
        wallet: &Arc<Wallet>,
        state: Option<&str>,
        current_daa_score: Option<u64>,
        include_utxos: bool,
        history: bool,
        account: Option<Arc<dyn runtime::Account>>,
    ) -> Vec<String> {
        let TransactionRecord { id, binding, block_daa_score, transaction_data, .. } = self;

        let name = match binding {
            Binding::Custom(id) => style(id.short()).cyan(),
            Binding::Account(account_id) => {
                let account = if let Some(account) = account {
                    Some(account)
                } else {
                    wallet.get_account_by_id(account_id).await.ok().flatten()
                };

                if let Some(account) = account {
                    style(account.name_with_id()).cyan()
                } else {
                    style(account_id.short() + " ??").magenta()
                }
            }
        };

        let transaction_type = transaction_data.transaction_type();
        let kind = transaction_type.style(&transaction_type.to_string());

        let maturity = current_daa_score
            .map(|score| {
                // TODO - refactor @ high BPS processing
                let maturity = if self.is_coinbase() {
                    crate::utxo::UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA.load(Ordering::SeqCst)
                } else {
                    crate::utxo::UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA.load(Ordering::SeqCst)
                };

                if score < self.block_daa_score() + maturity {
                    style("pending").dim().to_string()
                } else {
                    style("confirmed").dim().to_string()
                }
            })
            .unwrap_or_default();

        let block_daa_score = block_daa_score.separated_string();
        let state = state.unwrap_or(&maturity);
        let mut lines = vec![format!("{name} {id} @{block_daa_score} DAA - {kind} {state}")];

        let suffix = utils::kaspa_suffix(&self.network_id.network_type);

        match transaction_data {
            TransactionData::Reorg { utxo_entries, aggregate_input_value }
            | TransactionData::Incoming { utxo_entries, aggregate_input_value }
            | TransactionData::External { utxo_entries, aggregate_input_value } => {
                let aggregate_input_value =
                    transaction_type.style_with_sign(utils::sompi_to_kaspa_string(*aggregate_input_value).as_str(), history);
                lines.push(format!("{:>4}UTXOs: {}  Total: {}", "", utxo_entries.len(), aggregate_input_value));
                if include_utxos {
                    for utxo_entry in utxo_entries {
                        let address =
                            style(utxo_entry.address.as_ref().map(|addr| addr.to_string()).unwrap_or_else(|| "n/a".to_string()))
                                .blue();
                        let index = utxo_entry.index;
                        let is_coinbase = if utxo_entry.is_coinbase {
                            style(format!("coinbase utxo [{index}]")).dim()
                        } else {
                            style(format!("standard utxo [{index}]")).dim()
                        };
                        let amount =
                            transaction_type.style_with_sign(utils::sompi_to_kaspa_string(utxo_entry.amount).as_str(), history);

                        lines.push(format!("{:>4}{address}", ""));
                        lines.push(format!("{:>4}{amount} {suffix} {is_coinbase}", ""));
                    }
                }
            }
            TransactionData::Outgoing { fees, aggregate_input_value, transaction, payment_value, change_value, .. } => {
                if let Some(payment_value) = payment_value {
                    lines.push(format!(
                        "{:>4}Payment: {}  Used: {}  Fees: {}  Change: {}  UTXOs: [{}↠{}]",
                        "",
                        style(utils::sompi_to_kaspa_string(*payment_value)).red(),
                        style(utils::sompi_to_kaspa_string(*aggregate_input_value)).blue(),
                        style(utils::sompi_to_kaspa_string(*fees)).red(),
                        style(utils::sompi_to_kaspa_string(*change_value)).green(),
                        transaction.inputs.len(),
                        transaction.outputs.len(),
                    ));
                } else {
                    lines.push(format!(
                        "{:>4}Sweep: {}  Fees: {}  Change: {}  UTXOs: [{}↠{}]",
                        "",
                        style(utils::sompi_to_kaspa_string(*aggregate_input_value)).blue(),
                        style(utils::sompi_to_kaspa_string(*fees)).red(),
                        style(utils::sompi_to_kaspa_string(*change_value)).green(),
                        transaction.inputs.len(),
                        transaction.outputs.len(),
                    ));
                }

                if include_utxos {
                    for (_n, input) in transaction.inputs.iter().enumerate() {
                        let TransactionInput { previous_outpoint, signature_script: _, sequence, sig_op_count } = input;
                        let TransactionOutpoint { transaction_id, index } = previous_outpoint;

                        lines.push(format!("{:>4}{sequence:>2}: {transaction_id}:{index} SigOps: {sig_op_count}", ""));
                        // lines.push(format!("{:>4}{:>2}  Sig Ops: {sig_op_count}", "", ""));
                        // lines.push(format!("{:>4}{:>2}   Script: {}", "", "", signature_script.to_hex()));
                    }
                }
            }
        }

        lines
    }
}
