use crate::imports::*;
// use crate::runtime::Wallet;
use crate::storage::Binding;
use crate::tx::PendingTransactionInner;
use crate::utxo::{Maturity, OutgoingTransaction, UtxoContext, UtxoEntryReference};
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction};
// use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint};
use separator::Separatable;
use serde::{Deserialize, Serialize};
use workflow_core::time::{unixtime_as_millis_u64, unixtime_to_locale_string};
// use workflow_log::style;

pub use kaspa_consensus_core::tx::TransactionId;
use zeroize::Zeroize;

const TRANSACTION_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum TransactionKind {
    /// Incoming transaction
    Incoming,
    /// Transaction created by the runtime
    Outgoing,
    /// Outgoing transaction observed by the runtime
    External,
    /// Internal batch (sweep) transaction
    Batch,
    /// Reorg transaction (caused by UTXO reorg)
    Reorg,
    /// Stasis transaction (caused by reorg during coinbase UTXO stasis)
    /// NOTE: These types of transactions should be ignored by clients.
    Stasis,
    TransferIncoming,
    TransferOutgoing,
    Change,
}

impl TransactionKind {}

impl TransactionKind {
    pub fn sign(&self) -> String {
        match self {
            TransactionKind::Incoming => "+",
            TransactionKind::Outgoing => "-",
            TransactionKind::External => "-",
            TransactionKind::Batch => "",
            TransactionKind::Reorg => "-",
            TransactionKind::Stasis => "",
            TransactionKind::TransferIncoming => "",
            TransactionKind::TransferOutgoing => "",
            TransactionKind::Change => "",
        }
        .to_string()
    }
}

impl std::fmt::Display for TransactionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TransactionKind::Incoming => "incoming",
            TransactionKind::Outgoing => "outgoing",
            TransactionKind::External => "external",
            TransactionKind::Batch => "batch",
            TransactionKind::Reorg => "reorg",
            TransactionKind::Stasis => "stasis",
            TransactionKind::TransferIncoming => "transfer-incoming",
            TransactionKind::TransferOutgoing => "transfer-outgoing",
            TransactionKind::Change => "change",
        };
        write!(f, "{s}")
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum TransactionMetadata {
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "type", content = "transaction")]
// the reason the struct is renamed kebab-case and then
// each field is renamed to camelCase is to force the
// enum tags to be lower case.
#[serde(rename_all = "kebab-case")]
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
    Stasis {
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
    Batch {
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
        #[serde(rename = "acceptedDaaScore")]
        accepted_daa_score: Option<u64>,
        #[serde(rename = "utxoEntries")]
        #[serde(default)]
        utxo_entries: Vec<UtxoRecord>,
    },
    Outgoing {
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
        #[serde(rename = "acceptedDaaScore")]
        accepted_daa_score: Option<u64>,
        #[serde(rename = "utxoEntries")]
        #[serde(default)]
        utxo_entries: Vec<UtxoRecord>,
    },
    TransferIncoming {
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
        #[serde(rename = "acceptedDaaScore")]
        accepted_daa_score: Option<u64>,
        #[serde(rename = "utxoEntries")]
        utxo_entries: Vec<UtxoRecord>,
    },
    TransferOutgoing {
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
        #[serde(rename = "acceptedDaaScore")]
        accepted_daa_score: Option<u64>,
        #[serde(rename = "utxoEntries")]
        utxo_entries: Vec<UtxoRecord>,
    },
    Change {
        #[serde(rename = "inputValue")]
        aggregate_input_value: u64,
        #[serde(rename = "outputValue")]
        aggregate_output_value: u64,
        transaction: Transaction,
        #[serde(rename = "paymentValue")]
        payment_value: Option<u64>,
        #[serde(rename = "changeValue")]
        change_value: u64,
        #[serde(rename = "acceptedDaaScore")]
        accepted_daa_score: Option<u64>,
        #[serde(rename = "utxoEntries")]
        utxo_entries: Vec<UtxoRecord>,
    },
}

impl TransactionData {
    pub fn kind(&self) -> TransactionKind {
        match self {
            TransactionData::Reorg { .. } => TransactionKind::Reorg,
            TransactionData::Stasis { .. } => TransactionKind::Stasis,
            TransactionData::Incoming { .. } => TransactionKind::Incoming,
            TransactionData::External { .. } => TransactionKind::External,
            TransactionData::Outgoing { .. } => TransactionKind::Outgoing,
            TransactionData::Batch { .. } => TransactionKind::Batch,
            TransactionData::TransferIncoming { .. } => TransactionKind::TransferIncoming,
            TransactionData::TransferOutgoing { .. } => TransactionKind::TransferOutgoing,
            TransactionData::Change { .. } => TransactionKind::Change,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TransactionRecord {
    pub version: u16,
    pub id: TransactionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unixtime: Option<u64>,
    // TODO - remove default
    #[serde(default)]
    pub value: u64,
    pub binding: Binding,
    #[serde(rename = "blockDaaScore")]
    pub block_daa_score: u64,
    #[serde(rename = "network")]
    pub network_id: NetworkId,
    #[serde(rename = "data")]
    pub transaction_data: TransactionData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<TransactionMetadata>,
}

impl TransactionRecord {
    pub fn id(&self) -> &TransactionId {
        &self.id
    }

    pub fn unixtime(&self) -> Option<u64> {
        self.unixtime
    }

    pub fn unixtime_as_locale_string(&self) -> Option<String> {
        self.unixtime.map(unixtime_to_locale_string)
    }

    pub fn unixtime_or_daa_as_string(&self) -> String {
        if let Some(unixtime) = self.unixtime {
            unixtime_to_locale_string(unixtime)
        } else {
            self.block_daa_score.separated_string()
        }
    }

    pub fn set_unixtime(&mut self, unixtime: u64) {
        self.unixtime = Some(unixtime);
    }

    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    pub fn block_daa_score(&self) -> u64 {
        self.block_daa_score
    }

    pub fn maturity(&self, current_daa_score: u64) -> Maturity {
        // TODO - refactor @ high BPS processing
        let maturity = if self.is_coinbase() {
            crate::utxo::UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA.load(Ordering::SeqCst)
        } else {
            crate::utxo::UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA.load(Ordering::SeqCst)
        };

        if current_daa_score < self.block_daa_score() + maturity {
            Maturity::Pending
        } else {
            Maturity::Confirmed
        }
    }

    pub fn kind(&self) -> TransactionKind {
        self.transaction_data.kind()
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

    pub fn is_outgoing(&self) -> bool {
        matches!(&self.transaction_data, TransactionData::Outgoing { .. })
    }

    pub fn is_change(&self) -> bool {
        matches!(&self.transaction_data, TransactionData::Change { .. })
    }

    pub fn is_batch(&self) -> bool {
        matches!(&self.transaction_data, TransactionData::Batch { .. })
    }

    pub fn is_transfer(&self) -> bool {
        matches!(&self.transaction_data, TransactionData::TransferIncoming { .. } | TransactionData::TransferOutgoing { .. })
    }

    pub fn transaction_data(&self) -> &TransactionData {
        &self.transaction_data
    }

    // Transaction maturity ignores the stasis period and provides
    // a progress value based on the pending period. It is assumed
    // that transactions in stasis are not visible to the user.
    pub fn maturity_progress(&self, current_daa_score: u64) -> Option<f64> {
        let maturity = if self.is_coinbase() {
            crate::utxo::UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA.load(Ordering::SeqCst)
        } else {
            crate::utxo::UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA.load(Ordering::SeqCst)
        };

        if current_daa_score < self.block_daa_score + maturity {
            Some((current_daa_score - self.block_daa_score) as f64 / maturity as f64)
        } else {
            None
        }
    }

    pub fn aggregate_input_value(&self) -> u64 {
        match &self.transaction_data {
            TransactionData::Reorg { aggregate_input_value, .. }
            | TransactionData::Stasis { aggregate_input_value, .. }
            | TransactionData::Incoming { aggregate_input_value, .. }
            | TransactionData::External { aggregate_input_value, .. }
            | TransactionData::Outgoing { aggregate_input_value, .. }
            | TransactionData::Batch { aggregate_input_value, .. }
            | TransactionData::TransferIncoming { aggregate_input_value, .. }
            | TransactionData::TransferOutgoing { aggregate_input_value, .. }
            | TransactionData::Change { aggregate_input_value, .. } => *aggregate_input_value,
        }
    }

    pub fn value(&self) -> u64 {
        self.value
    }
}

impl TransactionRecord {
    pub fn new_incoming(utxo_context: &UtxoContext, id: TransactionId, utxos: &[UtxoEntryReference]) -> Self {
        Self::new_incoming_impl(utxo_context, TransactionKind::Incoming, id, utxos)
    }

    pub fn new_reorg(utxo_context: &UtxoContext, id: TransactionId, utxos: &[UtxoEntryReference]) -> Self {
        Self::new_incoming_impl(utxo_context, TransactionKind::Reorg, id, utxos)
    }
    pub fn new_stasis(utxo_context: &UtxoContext, id: TransactionId, utxos: &[UtxoEntryReference]) -> Self {
        Self::new_incoming_impl(utxo_context, TransactionKind::Stasis, id, utxos)
    }

    fn new_incoming_impl(
        utxo_context: &UtxoContext,
        transaction_type: TransactionKind,
        id: TransactionId,
        utxos: &[UtxoEntryReference],
    ) -> Self {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score = utxos[0].utxo.entry.block_daa_score;
        let utxo_entries = utxos.iter().map(UtxoRecord::from).collect::<Vec<_>>();
        let aggregate_input_value = utxo_entries.iter().map(|utxo| utxo.amount).sum::<u64>();

        let unixtime = unixtime_as_millis_u64();

        let transaction_data = match transaction_type {
            TransactionKind::Incoming => TransactionData::Incoming { utxo_entries, aggregate_input_value },
            TransactionKind::Reorg => TransactionData::Reorg { utxo_entries, aggregate_input_value },
            TransactionKind::Stasis => TransactionData::Stasis { utxo_entries, aggregate_input_value },
            kind => panic!("TransactionRecord::new_incoming() - invalid transaction type: {kind:?}"),
        };

        TransactionRecord {
            version: TRANSACTION_VERSION,
            id,
            unixtime: Some(unixtime),
            value: aggregate_input_value,
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        }
    }

    /// Transaction that was not issued by this instance of the wallet
    /// but belongs to this address set. This is an "external" transaction
    /// that occurs during the lifetime of this wallet.
    pub fn new_external(utxo_context: &UtxoContext, id: TransactionId, utxos: &[UtxoEntryReference]) -> Self {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score = utxos[0].utxo.entry.block_daa_score;
        let utxo_entries = utxos.iter().map(UtxoRecord::from).collect::<Vec<_>>();
        let aggregate_input_value = utxo_entries.iter().map(|utxo| utxo.amount).sum::<u64>();

        let transaction_data = TransactionData::External { utxo_entries, aggregate_input_value };
        let unixtime = unixtime_as_millis_u64();

        TransactionRecord {
            version: TRANSACTION_VERSION,
            id,
            unixtime: Some(unixtime),
            value: aggregate_input_value,
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        }
    }

    pub fn new_outgoing(utxo_context: &UtxoContext, outgoing_tx: &OutgoingTransaction, accepted_daa_score: Option<u64>) -> Self {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score =
            utxo_context.processor().current_daa_score().expect("TransactionRecord::new_outgoing() - missing daa score");

        let utxo_entries = outgoing_tx.utxo_entries().into_iter().map(UtxoRecord::from).collect::<Vec<_>>();

        let unixtime = unixtime_as_millis_u64();

        let PendingTransactionInner {
            signable_tx,
            fees,
            aggregate_input_value,
            aggregate_output_value,
            payment_value,
            change_output_value,
            ..
        } = &*outgoing_tx.pending_transaction().inner;

        let transaction = signable_tx.lock().unwrap().tx.clone();
        let id = transaction.id();

        let transaction_data = TransactionData::Outgoing {
            fees: *fees,
            aggregate_input_value: *aggregate_input_value,
            aggregate_output_value: *aggregate_output_value,
            transaction,
            payment_value: *payment_value,
            change_value: *change_output_value,
            accepted_daa_score,
            utxo_entries,
        };

        TransactionRecord {
            version: TRANSACTION_VERSION,
            id,
            unixtime: Some(unixtime),
            value: payment_value.unwrap_or(*aggregate_input_value),
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        }
    }

    pub fn new_batch(utxo_context: &UtxoContext, outgoing_tx: &OutgoingTransaction, accepted_daa_score: Option<u64>) -> Self {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score =
            utxo_context.processor().current_daa_score().expect("TransactionRecord::new_outgoing() - missing daa score");

        let utxo_entries = outgoing_tx.utxo_entries().into_iter().map(UtxoRecord::from).collect::<Vec<_>>();

        let unixtime = unixtime_as_millis_u64();

        let PendingTransactionInner {
            signable_tx,
            fees,
            aggregate_input_value,
            aggregate_output_value,
            payment_value,
            change_output_value,
            ..
        } = &*outgoing_tx.pending_transaction().inner;

        let transaction = signable_tx.lock().unwrap().tx.clone();
        let id = transaction.id();

        let transaction_data = TransactionData::Batch {
            fees: *fees,
            aggregate_input_value: *aggregate_input_value,
            aggregate_output_value: *aggregate_output_value,
            transaction,
            payment_value: *payment_value,
            change_value: *change_output_value,
            accepted_daa_score,
            utxo_entries,
        };

        TransactionRecord {
            version: TRANSACTION_VERSION,
            id,
            unixtime: Some(unixtime),
            value: payment_value.unwrap_or(*aggregate_input_value),
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        }
    }

    pub fn new_transfer_incoming(
        utxo_context: &UtxoContext,
        outgoing_tx: &OutgoingTransaction,
        accepted_daa_score: Option<u64>,
        utxos: &[UtxoEntryReference],
    ) -> Self {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score =
            utxo_context.processor().current_daa_score().expect("TransactionRecord::new_outgoing() - missing daa score");
        let utxo_entries = utxos.iter().map(UtxoRecord::from).collect::<Vec<_>>();

        let unixtime = unixtime_as_millis_u64();

        let PendingTransactionInner {
            signable_tx,
            fees,
            aggregate_input_value,
            aggregate_output_value,
            payment_value,
            change_output_value,
            ..
        } = &*outgoing_tx.pending_transaction().inner;

        let transaction = signable_tx.lock().unwrap().tx.clone();
        let id = transaction.id();

        let transaction_data = TransactionData::TransferIncoming {
            fees: *fees,
            aggregate_input_value: *aggregate_input_value,
            aggregate_output_value: *aggregate_output_value,
            transaction,
            payment_value: *payment_value,
            change_value: *change_output_value,
            accepted_daa_score,
            utxo_entries,
        };

        TransactionRecord {
            version: TRANSACTION_VERSION,
            id,
            unixtime: Some(unixtime),
            value: payment_value.unwrap_or(*aggregate_input_value),
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        }
    }

    pub fn new_transfer_outgoing(
        utxo_context: &UtxoContext,
        outgoing_tx: &OutgoingTransaction,
        accepted_daa_score: Option<u64>,
        utxos: &[UtxoEntryReference],
    ) -> Self {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score =
            utxo_context.processor().current_daa_score().expect("TransactionRecord::new_outgoing() - missing daa score");
        let utxo_entries = utxos.iter().map(UtxoRecord::from).collect::<Vec<_>>();

        let unixtime = unixtime_as_millis_u64();

        let PendingTransactionInner {
            signable_tx,
            fees,
            aggregate_input_value,
            aggregate_output_value,
            payment_value,
            change_output_value,
            ..
        } = &*outgoing_tx.pending_transaction().inner;

        let transaction = signable_tx.lock().unwrap().tx.clone();
        let id = transaction.id();

        let transaction_data = TransactionData::TransferOutgoing {
            fees: *fees,
            aggregate_input_value: *aggregate_input_value,
            aggregate_output_value: *aggregate_output_value,
            transaction,
            payment_value: *payment_value,
            change_value: *change_output_value,
            accepted_daa_score,
            utxo_entries,
        };

        TransactionRecord {
            version: TRANSACTION_VERSION,
            id,
            unixtime: Some(unixtime),
            value: payment_value.unwrap_or(*aggregate_input_value),
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        }
    }

    pub fn new_change(
        utxo_context: &UtxoContext,
        outgoing_tx: &OutgoingTransaction,
        accepted_daa_score: Option<u64>,
        utxos: &[UtxoEntryReference],
    ) -> Self {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score =
            utxo_context.processor().current_daa_score().expect("TransactionRecord::new_outgoing() - missing daa score");
        let utxo_entries = utxos.into_iter().map(UtxoRecord::from).collect::<Vec<_>>();

        let unixtime = unixtime_as_millis_u64();

        let PendingTransactionInner {
            signable_tx,
            aggregate_input_value,
            aggregate_output_value,
            payment_value,
            change_output_value,
            ..
        } = &*outgoing_tx.pending_transaction().inner;

        let transaction = signable_tx.lock().unwrap().tx.clone();
        let id = transaction.id();

        let transaction_data = TransactionData::Change {
            aggregate_input_value: *aggregate_input_value,
            aggregate_output_value: *aggregate_output_value,
            transaction,
            payment_value: *payment_value,
            change_value: *change_output_value,
            accepted_daa_score,
            utxo_entries,
        };

        TransactionRecord {
            version: TRANSACTION_VERSION,
            id,
            unixtime: Some(unixtime),
            value: *change_output_value,
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        }
    }
}

impl Zeroize for TransactionRecord {
    fn zeroize(&mut self) {
        // TODO - this trait is added due to the
        // Encryptable<TransactionRecord> requirement
        // for T to be Zeroize.
        //
        // This will be updated later
        //
        // self.id.zeroize();
        // self.binding.zeroize();
        // self.block_daa_score.zeroize();
        // self.transaction_data.zeroize();
        // self.network_id.zeroize();
        // self.metadata.zeroize();
    }
}
