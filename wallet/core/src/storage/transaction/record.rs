//!
//! Wallet transaction record implementation.
//!

use super::*;
use crate::imports::*;
use crate::storage::Binding;
use crate::tx::PendingTransactionInner;
use workflow_core::time::{unixtime_as_millis_u64, unixtime_to_locale_string};
use workflow_wasm::utils::try_get_js_value_prop;

pub use kaspa_consensus_core::tx::TransactionId;
use zeroize::Zeroize;

#[wasm_bindgen(typescript_custom_section)]
const ITransactionRecord: &'static str = r#"

/**
 * 
 * @category Wallet SDK
 */
export interface IUtxoRecord {
    address?: Address;
    index: number;
    amount: bigint;
    scriptPublicKey: HexString;
    isCoinbase: boolean;
}

/**
 * Type of transaction data record.
 * @see {@link ITransactionData}, {@link ITransactionDataVariant}, {@link ITransactionRecord}
 * @category Wallet SDK
 */
export enum TransactionDataType {
    /**
     * Transaction has been invalidated due to a BlockDAG reorganization.
     * Such transaction is no longer valid and its UTXO entries are removed.
     * @see {@link ITransactionDataReorg}
     */
    Reorg = "reorg",
    /**
     * Transaction has been received and its UTXO entries are added to the 
     * pending or mature UTXO set.
     * @see {@link ITransactionDataIncoming}
     */
    Incoming = "incoming",
    /**
     * Transaction is in stasis and its UTXO entries are not yet added to the UTXO set.
     * This event is generated for **Coinbase** transactions only.
     * @see {@link ITransactionDataStasis}
     */
    Stasis = "stasis",
    /**
     * Observed transaction is not performed by the wallet subsystem but is executed
     * against the address set managed by the wallet subsystem.
     * @see {@link ITransactionDataExternal}
     */
    External = "external",
    /**
     * Transaction is outgoing and its UTXO entries are removed from the UTXO set.
     * @see {@link ITransactionDataOutgoing}
     */
    Outgoing = "outgoing",
    /**
     * Transaction is a batch transaction (compounding UTXOs to an internal change address).
     * @see {@link ITransactionDataBatch}
     */
    Batch = "batch",
    /**
     * Transaction is an incoming transfer from another {@link UtxoContext} managed by the {@link UtxoProcessor}.
     * When operating under the integrated wallet, these are transfers between different wallet accounts.
     * @see {@link ITransactionDataTransferIncoming}
     */
    TransferIncoming = "transfer-incoming",
    /**
     * Transaction is an outgoing transfer to another {@link UtxoContext} managed by the {@link UtxoProcessor}.
     * When operating under the integrated wallet, these are transfers between different wallet accounts.
     * @see {@link ITransactionDataTransferOutgoing}
     */
    TransferOutgoing = "transfer-outgoing",
    /**
     * Transaction is a change transaction and its UTXO entries are added to the UTXO set.
     * @see {@link ITransactionDataChange}
     */
    Change = "change",
}

/**
 * Contains UTXO entries and value for a transaction
 * that has been invalidated due to a BlockDAG reorganization.
 * @category Wallet SDK
 */
export interface ITransactionDataReorg {
    utxoEntries: IUtxoRecord[];
    value: bigint;
}

/**
 * Contains UTXO entries and value for an incoming transaction.
 * @category Wallet SDK
 */
export interface ITransactionDataIncoming {
    utxoEntries: IUtxoRecord[];
    value: bigint;
}

/**
 * Contains UTXO entries and value for a stasis transaction.
 * @category Wallet SDK
 */
export interface ITransactionDataStasis {
    utxoEntries: IUtxoRecord[];
    value: bigint;
}

/**
 * Contains UTXO entries and value for an external transaction.
 * An external transaction is a transaction that was not issued 
 * by this instance of the wallet but belongs to this address set.
 * @category Wallet SDK
 */
export interface ITransactionDataExternal {
    utxoEntries: IUtxoRecord[];
    value: bigint;
}

/**
 * Batch transaction data (created by the {@link Generator} as a 
 * result of UTXO compounding process).
 * @category Wallet SDK
 */
export interface ITransactionDataBatch {
    fees: bigint;
    inputValue: bigint;
    outputValue: bigint;
    transaction: ITransaction;
    paymentValue: bigint;
    changeValue: bigint;
    acceptedDaaScore?: bigint;
    utxoEntries: IUtxoRecord[];
}

/**
 * Outgoing transaction data.
 * @category Wallet SDK
 */
export interface ITransactionDataOutgoing {
    fees: bigint;
    inputValue: bigint;
    outputValue: bigint;
    transaction: ITransaction;
    paymentValue: bigint;
    changeValue: bigint;
    acceptedDaaScore?: bigint;
    utxoEntries: IUtxoRecord[];
}

/**
 * Incoming transfer transaction data.
 * Transfer occurs when a transaction is issued between 
 * two {@link UtxoContext} (wallet account) instances.
 * @category Wallet SDK
 */
export interface ITransactionDataTransferIncoming {
    fees: bigint;
    inputValue: bigint;
    outputValue: bigint;
    transaction: ITransaction;
    paymentValue: bigint;
    changeValue: bigint;
    acceptedDaaScore?: bigint;
    utxoEntries: IUtxoRecord[];
}

/**
 * Outgoing transfer transaction data.
 * Transfer occurs when a transaction is issued between 
 * two {@link UtxoContext} (wallet account) instances.
 * @category Wallet SDK
 */
export interface ITransactionDataTransferOutgoing {
    fees: bigint;
    inputValue: bigint;
    outputValue: bigint;
    transaction: ITransaction;
    paymentValue: bigint;
    changeValue: bigint;
    acceptedDaaScore?: bigint;
    utxoEntries: IUtxoRecord[];
}

/**
 * Change transaction data.
 * @category Wallet SDK
 */
export interface ITransactionDataChange {
    inputValue: bigint;
    outputValue: bigint;
    transaction: ITransaction;
    paymentValue: bigint;
    changeValue: bigint;
    acceptedDaaScore?: bigint;
    utxoEntries: IUtxoRecord[];
}

/**
 * Transaction record data variants.
 * @category Wallet SDK
 */
export type ITransactionDataVariant = 
    ITransactionDataReorg
    | ITransactionDataIncoming
    | ITransactionDataStasis
    | ITransactionDataExternal
    | ITransactionDataOutgoing
    | ITransactionDataBatch
    | ITransactionDataTransferIncoming
    | ITransactionDataTransferOutgoing
    | ITransactionDataChange;

/**
 * Internal transaction data contained within the transaction record.
 * @see {@link ITransactionRecord}
 * @category Wallet SDK
 */
export interface ITransactionData {
    type : TransactionDataType;
    data : ITransactionDataVariant;
}

/**
 * Transaction record generated by the Kaspa Wallet SDK.
 * This data structure is delivered within {@link UtxoProcessor} and `Wallet` notification events.
 * @see {@link ITransactionData}, {@link TransactionDataType}, {@link ITransactionDataVariant}
 * @category Wallet SDK
 */
export interface ITransactionRecord {
    /**
     * Transaction id.
     */
    id: string;
    /**
     * Transaction UNIX time in milliseconds.
     */
    unixtimeMsec?: bigint;
    /**
     * Transaction value in SOMPI.
     */
    value: bigint;
    /**
     * Transaction binding (id of UtxoContext or Wallet Account).
     */
    binding: HexString;
    /**
     * Block DAA score.
     */
    blockDaaScore: bigint;
    /**
     * Network id on which this transaction has occurred.
     */
    network: NetworkId;
    /**
     * Transaction data.
     */
    data: ITransactionData;
    /**
     * Optional transaction note as a human-readable string.
     */
    note?: string;
    /**
     * Optional transaction metadata.
     * 
     * If present, this must contain a JSON-serialized string.
     * A client application updating the metadata must deserialize
     * the string into JSON, add a key with it's own identifier
     * and store its own metadata into the value of this key.
     */
    metadata?: string;

    /**
     * Transaction data type.
     */
    type: string;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = Object, typescript_type = "ITransactionRecord")]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub type ITransactionRecord;
}

#[wasm_bindgen(inspectable)]
#[derive(Debug, Clone, Serialize)]
pub struct TransactionRecordNotification {
    #[serde(rename = "type")]
    #[wasm_bindgen(js_name = "type", getter_with_clone)]
    pub type_: String,
    #[wasm_bindgen(getter_with_clone)]
    pub data: TransactionRecord,
}

impl TransactionRecordNotification {
    pub fn new(type_: String, data: TransactionRecord) -> Self {
        Self { type_, data }
    }
}

/// @category Wallet SDK
#[wasm_bindgen(inspectable)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub id: TransactionId,
    /// Unix time in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "unixtimeMsec")]
    #[wasm_bindgen(js_name = unixtimeMsec)]
    pub unixtime_msec: Option<u64>,
    pub value: u64,
    #[wasm_bindgen(skip)]
    pub binding: Binding,
    #[serde(rename = "blockDaaScore")]
    #[wasm_bindgen(js_name = blockDaaScore)]
    pub block_daa_score: u64,
    #[serde(rename = "network")]
    #[wasm_bindgen(js_name = network)]
    pub network_id: NetworkId,
    #[serde(rename = "data")]
    #[wasm_bindgen(skip)]
    pub transaction_data: TransactionData,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[wasm_bindgen(getter_with_clone)]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[wasm_bindgen(getter_with_clone)]
    pub metadata: Option<String>,
}

impl TransactionRecord {
    const STORAGE_MAGIC: u32 = 0x5854414b;
    const STORAGE_VERSION: u32 = 0;

    pub fn id(&self) -> &TransactionId {
        &self.id
    }

    pub fn unixtime_msec(&self) -> Option<u64> {
        self.unixtime_msec
    }

    pub fn unixtime_as_locale_string(&self) -> Option<String> {
        self.unixtime_msec.map(unixtime_to_locale_string)
    }

    pub fn unixtime_or_daa_as_string(&self) -> String {
        if let Some(unixtime) = self.unixtime_msec {
            unixtime_to_locale_string(unixtime)
        } else {
            self.block_daa_score.separated_string()
        }
    }

    pub fn set_unixtime(&mut self, unixtime: u64) {
        self.unixtime_msec = Some(unixtime);
    }

    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    pub fn block_daa_score(&self) -> u64 {
        self.block_daa_score
    }

    pub fn maturity(&self, current_daa_score: u64) -> Maturity {
        let params = NetworkParams::from(self.network_id);

        let maturity = if self.is_coinbase() {
            params.coinbase_transaction_maturity_period_daa
        } else {
            params.user_transaction_maturity_period_daa
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
        let params = NetworkParams::from(self.network_id);
        let maturity = if self.is_coinbase() {
            params.coinbase_transaction_maturity_period_daa
        } else {
            params.user_transaction_maturity_period_daa
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
        let block_daa_score = utxos[0].utxo.block_daa_score;
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
            id,
            unixtime_msec: Some(unixtime),
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
        let block_daa_score = utxos[0].utxo.block_daa_score;
        let utxo_entries = utxos.iter().map(UtxoRecord::from).collect::<Vec<_>>();
        let aggregate_input_value = utxo_entries.iter().map(|utxo| utxo.amount).sum::<u64>();

        let transaction_data = TransactionData::External { utxo_entries, aggregate_input_value };
        let unixtime = unixtime_as_millis_u64();

        TransactionRecord {
            id,
            unixtime_msec: Some(unixtime),
            value: aggregate_input_value,
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        }
    }

    pub fn new_outgoing(
        utxo_context: &UtxoContext,
        outgoing_tx: &OutgoingTransaction,
        accepted_daa_score: Option<u64>,
    ) -> Result<Self> {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score =
            utxo_context.processor().current_daa_score().ok_or(Error::MissingDaaScore("TransactionRecord::new_outgoing()"))?;

        let utxo_entries = outgoing_tx.utxo_entries().values().map(UtxoRecord::from).collect::<Vec<_>>();

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

        Ok(TransactionRecord {
            id,
            unixtime_msec: Some(unixtime),
            value: payment_value.unwrap_or(*aggregate_input_value),
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        })
    }

    pub fn new_batch(utxo_context: &UtxoContext, outgoing_tx: &OutgoingTransaction, accepted_daa_score: Option<u64>) -> Result<Self> {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score =
            utxo_context.processor().current_daa_score().ok_or(Error::MissingDaaScore("TransactionRecord::new_batch()"))?;

        let utxo_entries = outgoing_tx.utxo_entries().values().map(UtxoRecord::from).collect::<Vec<_>>();

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

        Ok(TransactionRecord {
            id,
            unixtime_msec: Some(unixtime),
            value: payment_value.unwrap_or(*aggregate_input_value),
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        })
    }

    pub fn new_transfer_incoming(
        utxo_context: &UtxoContext,
        outgoing_tx: &OutgoingTransaction,
        accepted_daa_score: Option<u64>,
        utxos: &[UtxoEntryReference],
    ) -> Result<Self> {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score = utxo_context
            .processor()
            .current_daa_score()
            .ok_or(Error::MissingDaaScore("TransactionRecord::new_transfer_incoming()"))?;
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

        Ok(TransactionRecord {
            id,
            unixtime_msec: Some(unixtime),
            value: payment_value.unwrap_or(*aggregate_input_value),
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        })
    }

    pub fn new_transfer_outgoing(
        utxo_context: &UtxoContext,
        outgoing_tx: &OutgoingTransaction,
        accepted_daa_score: Option<u64>,
        utxos: &[UtxoEntryReference],
    ) -> Result<Self> {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score = utxo_context
            .processor()
            .current_daa_score()
            .ok_or(Error::MissingDaaScore("TransactionRecord::new_transfer_outgoing()"))?;
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

        Ok(TransactionRecord {
            id,
            unixtime_msec: Some(unixtime),
            value: payment_value.unwrap_or(*aggregate_input_value),
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        })
    }

    pub fn new_change(
        utxo_context: &UtxoContext,
        outgoing_tx: &OutgoingTransaction,
        accepted_daa_score: Option<u64>,
        utxos: &[UtxoEntryReference],
    ) -> Result<Self> {
        let binding = Binding::from(utxo_context.binding());
        let block_daa_score =
            utxo_context.processor().current_daa_score().ok_or(Error::MissingDaaScore("TransactionRecord::new_change()"))?;
        let utxo_entries = utxos.iter().map(UtxoRecord::from).collect::<Vec<_>>();

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

        Ok(TransactionRecord {
            id,
            unixtime_msec: Some(unixtime),
            value: *change_output_value,
            binding,
            transaction_data,
            block_daa_score,
            network_id: utxo_context.processor().network_id().expect("network expected for transaction record generation"),
            metadata: None,
            note: None,
        })
    }
}

#[wasm_bindgen]
impl TransactionRecord {
    #[wasm_bindgen(getter, js_name = "binding")]
    pub fn binding_as_js_value(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.binding).unwrap()
    }

    #[wasm_bindgen(getter, js_name = "data")]
    pub fn data_as_js_value(&self) -> JsValue {
        try_get_js_value_prop(&serde_wasm_bindgen::to_value(&self.transaction_data).unwrap(), "data").unwrap()
    }

    #[wasm_bindgen(getter, js_name = "type")]
    pub fn data_type(&self) -> String {
        self.transaction_data.kind().to_string()
    }

    /// Check if the transaction record has the given address within the associated UTXO set.
    #[wasm_bindgen(js_name = hasAddress)]
    pub fn has_address(&self, address: &Address) -> bool {
        self.transaction_data.has_address(address)
    }

    /// Serialize the transaction record to a JavaScript object.
    pub fn serialize(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self).unwrap()
    }
}

impl Zeroize for TransactionRecord {
    fn zeroize(&mut self) {
        // TODO - this trait is added due to the
        // Encryptable<TransactionRecord> requirement
        // for T to be Zeroize.
    }
}

impl BorshSerialize for TransactionRecord {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;
        BorshSerialize::serialize(&self.id, writer)?;
        BorshSerialize::serialize(&self.unixtime_msec, writer)?;
        BorshSerialize::serialize(&self.value, writer)?;
        BorshSerialize::serialize(&self.binding, writer)?;
        BorshSerialize::serialize(&self.block_daa_score, writer)?;
        BorshSerialize::serialize(&self.network_id, writer)?;
        BorshSerialize::serialize(&self.transaction_data, writer)?;
        BorshSerialize::serialize(&self.note, writer)?;
        BorshSerialize::serialize(&self.metadata, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for TransactionRecord {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize(buf)?.try_magic(Self::STORAGE_MAGIC)?.try_version(Self::STORAGE_VERSION)?;

        let id = BorshDeserialize::deserialize(buf)?;
        let unixtime = BorshDeserialize::deserialize(buf)?;
        let value = BorshDeserialize::deserialize(buf)?;
        let binding = BorshDeserialize::deserialize(buf)?;
        let block_daa_score = BorshDeserialize::deserialize(buf)?;
        let network_id = BorshDeserialize::deserialize(buf)?;
        let transaction_data = BorshDeserialize::deserialize(buf)?;
        let note = BorshDeserialize::deserialize(buf)?;
        let metadata = BorshDeserialize::deserialize(buf)?;

        Ok(Self { id, unixtime_msec: unixtime, value, binding, block_daa_score, network_id, transaction_data, note, metadata })
    }
}

// impl From<TransactionRecord> for JsValue {
//     fn from(record: TransactionRecord) -> Self {
//         serde_wasm_bindgen::to_value(&record).unwrap()
//     }
// }

impl From<TransactionRecord> for ITransactionRecord {
    fn from(record: TransactionRecord) -> Self {
        JsValue::from(record).unchecked_into()
    }
}
