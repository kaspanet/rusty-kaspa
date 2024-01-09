//!
//! Wallet transaction record implementation.
//!

use super::*;
use crate::imports::*;
use crate::storage::Binding;
use crate::tx::PendingTransactionInner;
use workflow_core::time::{unixtime_as_millis_u64, unixtime_to_locale_string};

pub use kaspa_consensus_core::tx::TransactionId;
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub id: TransactionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "unixtimeMsec")]
    pub unixtime_msec: Option<u64>,
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
        let block_daa_score = utxos[0].utxo.entry.block_daa_score;
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

impl TryFrom<JsValue> for TransactionRecord {
    type Error = Error;

    fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&value) {
            let transaction_record = Self {
                id: object.get_value("id")?.try_into()?,
                unixtime_msec: object.try_get_value("unixtimeMsec")?.map(|value| value.try_as_u64()).transpose()?,
                value: object.get_u64("value")?,
                binding: object.get_value("binding")?.try_into()?,
                block_daa_score: object.get_u64("blockDaaScore")?,
                network_id: object.get_value("networkId")?.try_into()?,
                transaction_data: object.get_value("transactionData")?.try_into()?,
                note: object.try_get_string("note")?,
                metadata: object.try_get_string("metadata")?,
            };

            Ok(transaction_record)
        } else {
            Err(Error::Custom("supplied argument must be an object".to_string()))
        }
    }
}
