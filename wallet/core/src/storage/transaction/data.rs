//!
//! Wallet transaction data variants.
//!

use super::UtxoRecord;
use crate::imports::*;
use kaspa_consensus_core::tx::Transaction;
pub use kaspa_consensus_core::tx::TransactionId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
// the reason the struct is renamed kebab-case and then
// each field is renamed to camelCase is to force the
// enum tags to be lower-kebab-case.
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
    const STORAGE_MAGIC: u32 = 0x54445854;
    const STORAGE_VERSION: u32 = 0;

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

    pub fn has_address(&self, address: &Address) -> bool {
        match self {
            TransactionData::Reorg { utxo_entries, .. } => utxo_entries.iter().any(|utxo| utxo.address.as_ref() == Some(address)),
            TransactionData::Stasis { utxo_entries, .. } => utxo_entries.iter().any(|utxo| utxo.address.as_ref() == Some(address)),
            TransactionData::Incoming { utxo_entries, .. } => utxo_entries.iter().any(|utxo| utxo.address.as_ref() == Some(address)),
            TransactionData::External { utxo_entries, .. } => utxo_entries.iter().any(|utxo| utxo.address.as_ref() == Some(address)),
            TransactionData::Outgoing { utxo_entries, .. } => utxo_entries.iter().any(|utxo| utxo.address.as_ref() == Some(address)),
            TransactionData::Batch { utxo_entries, .. } => utxo_entries.iter().any(|utxo| utxo.address.as_ref() == Some(address)),
            TransactionData::TransferIncoming { utxo_entries, .. } => {
                utxo_entries.iter().any(|utxo| utxo.address.as_ref() == Some(address))
            }
            TransactionData::TransferOutgoing { utxo_entries, .. } => {
                utxo_entries.iter().any(|utxo| utxo.address.as_ref() == Some(address))
            }
            TransactionData::Change { utxo_entries, .. } => utxo_entries.iter().any(|utxo| utxo.address.as_ref() == Some(address)),
        }
    }
}

impl BorshSerialize for TransactionData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;

        let kind = self.kind();
        BorshSerialize::serialize(&kind, writer)?;

        match self {
            TransactionData::Reorg { utxo_entries, aggregate_input_value } => {
                BorshSerialize::serialize(utxo_entries, writer)?;
                BorshSerialize::serialize(aggregate_input_value, writer)?;
            }
            TransactionData::Incoming { utxo_entries, aggregate_input_value } => {
                BorshSerialize::serialize(utxo_entries, writer)?;
                BorshSerialize::serialize(aggregate_input_value, writer)?;
            }
            TransactionData::Stasis { utxo_entries, aggregate_input_value } => {
                BorshSerialize::serialize(utxo_entries, writer)?;
                BorshSerialize::serialize(aggregate_input_value, writer)?;
            }
            TransactionData::External { utxo_entries, aggregate_input_value } => {
                BorshSerialize::serialize(utxo_entries, writer)?;
                BorshSerialize::serialize(aggregate_input_value, writer)?;
            }
            TransactionData::Batch {
                fees,
                aggregate_input_value,
                aggregate_output_value,
                transaction,
                payment_value,
                change_value,
                accepted_daa_score,
                utxo_entries,
            } => {
                BorshSerialize::serialize(fees, writer)?;
                BorshSerialize::serialize(aggregate_input_value, writer)?;
                BorshSerialize::serialize(aggregate_output_value, writer)?;
                BorshSerialize::serialize(transaction, writer)?;
                BorshSerialize::serialize(payment_value, writer)?;
                BorshSerialize::serialize(change_value, writer)?;
                BorshSerialize::serialize(accepted_daa_score, writer)?;
                BorshSerialize::serialize(utxo_entries, writer)?;
            }
            TransactionData::Outgoing {
                fees,
                aggregate_input_value,
                aggregate_output_value,
                transaction,
                payment_value,
                change_value,
                accepted_daa_score,
                utxo_entries,
            } => {
                BorshSerialize::serialize(fees, writer)?;
                BorshSerialize::serialize(aggregate_input_value, writer)?;
                BorshSerialize::serialize(aggregate_output_value, writer)?;
                BorshSerialize::serialize(transaction, writer)?;
                BorshSerialize::serialize(payment_value, writer)?;
                BorshSerialize::serialize(change_value, writer)?;
                BorshSerialize::serialize(accepted_daa_score, writer)?;
                BorshSerialize::serialize(utxo_entries, writer)?;
            }
            TransactionData::TransferIncoming {
                fees,
                aggregate_input_value,
                aggregate_output_value,
                transaction,
                payment_value,
                change_value,
                accepted_daa_score,
                utxo_entries,
            } => {
                BorshSerialize::serialize(fees, writer)?;
                BorshSerialize::serialize(aggregate_input_value, writer)?;
                BorshSerialize::serialize(aggregate_output_value, writer)?;
                BorshSerialize::serialize(transaction, writer)?;
                BorshSerialize::serialize(payment_value, writer)?;
                BorshSerialize::serialize(change_value, writer)?;
                BorshSerialize::serialize(accepted_daa_score, writer)?;
                BorshSerialize::serialize(utxo_entries, writer)?;
            }
            TransactionData::TransferOutgoing {
                fees,
                aggregate_input_value,
                aggregate_output_value,
                transaction,
                payment_value,
                change_value,
                accepted_daa_score,
                utxo_entries,
            } => {
                BorshSerialize::serialize(fees, writer)?;
                BorshSerialize::serialize(aggregate_input_value, writer)?;
                BorshSerialize::serialize(aggregate_output_value, writer)?;
                BorshSerialize::serialize(transaction, writer)?;
                BorshSerialize::serialize(payment_value, writer)?;
                BorshSerialize::serialize(change_value, writer)?;
                BorshSerialize::serialize(accepted_daa_score, writer)?;
                BorshSerialize::serialize(utxo_entries, writer)?;
            }
            TransactionData::Change {
                aggregate_input_value,
                aggregate_output_value,
                transaction,
                payment_value,
                change_value,
                accepted_daa_score,
                utxo_entries,
            } => {
                BorshSerialize::serialize(aggregate_input_value, writer)?;
                BorshSerialize::serialize(aggregate_output_value, writer)?;
                BorshSerialize::serialize(transaction, writer)?;
                BorshSerialize::serialize(payment_value, writer)?;
                BorshSerialize::serialize(change_value, writer)?;
                BorshSerialize::serialize(accepted_daa_score, writer)?;
                BorshSerialize::serialize(utxo_entries, writer)?;
            }
        }

        Ok(())
    }
}

impl BorshDeserialize for TransactionData {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize(buf)?.try_magic(Self::STORAGE_MAGIC)?.try_version(Self::STORAGE_VERSION)?;

        let kind: TransactionKind = BorshDeserialize::deserialize(buf)?;

        match kind {
            TransactionKind::Reorg => {
                let utxo_entries: Vec<UtxoRecord> = BorshDeserialize::deserialize(buf)?;
                let aggregate_input_value: u64 = BorshDeserialize::deserialize(buf)?;
                Ok(TransactionData::Reorg { utxo_entries, aggregate_input_value })
            }
            TransactionKind::Incoming => {
                let utxo_entries: Vec<UtxoRecord> = BorshDeserialize::deserialize(buf)?;
                let aggregate_input_value: u64 = BorshDeserialize::deserialize(buf)?;
                Ok(TransactionData::Incoming { utxo_entries, aggregate_input_value })
            }
            TransactionKind::Stasis => {
                let utxo_entries: Vec<UtxoRecord> = BorshDeserialize::deserialize(buf)?;
                let aggregate_input_value: u64 = BorshDeserialize::deserialize(buf)?;
                Ok(TransactionData::Stasis { utxo_entries, aggregate_input_value })
            }
            TransactionKind::External => {
                let utxo_entries: Vec<UtxoRecord> = BorshDeserialize::deserialize(buf)?;
                let aggregate_input_value: u64 = BorshDeserialize::deserialize(buf)?;
                Ok(TransactionData::External { utxo_entries, aggregate_input_value })
            }
            TransactionKind::Batch => {
                let fees: u64 = BorshDeserialize::deserialize(buf)?;
                let aggregate_input_value: u64 = BorshDeserialize::deserialize(buf)?;
                let aggregate_output_value: u64 = BorshDeserialize::deserialize(buf)?;
                let transaction: Transaction = BorshDeserialize::deserialize(buf)?;
                let payment_value: Option<u64> = BorshDeserialize::deserialize(buf)?;
                let change_value: u64 = BorshDeserialize::deserialize(buf)?;
                let accepted_daa_score: Option<u64> = BorshDeserialize::deserialize(buf)?;
                let utxo_entries: Vec<UtxoRecord> = BorshDeserialize::deserialize(buf)?;
                Ok(TransactionData::Batch {
                    fees,
                    aggregate_input_value,
                    aggregate_output_value,
                    transaction,
                    payment_value,
                    change_value,
                    accepted_daa_score,
                    utxo_entries,
                })
            }
            TransactionKind::Outgoing => {
                let fees: u64 = BorshDeserialize::deserialize(buf)?;
                let aggregate_input_value: u64 = BorshDeserialize::deserialize(buf)?;
                let aggregate_output_value: u64 = BorshDeserialize::deserialize(buf)?;
                let transaction: Transaction = BorshDeserialize::deserialize(buf)?;
                let payment_value: Option<u64> = BorshDeserialize::deserialize(buf)?;
                let change_value: u64 = BorshDeserialize::deserialize(buf)?;
                let accepted_daa_score: Option<u64> = BorshDeserialize::deserialize(buf)?;
                let utxo_entries: Vec<UtxoRecord> = BorshDeserialize::deserialize(buf)?;
                Ok(TransactionData::Outgoing {
                    fees,
                    aggregate_input_value,
                    aggregate_output_value,
                    transaction,
                    payment_value,
                    change_value,
                    accepted_daa_score,
                    utxo_entries,
                })
            }
            TransactionKind::TransferIncoming => {
                let fees: u64 = BorshDeserialize::deserialize(buf)?;
                let aggregate_input_value: u64 = BorshDeserialize::deserialize(buf)?;
                let aggregate_output_value: u64 = BorshDeserialize::deserialize(buf)?;
                let transaction: Transaction = BorshDeserialize::deserialize(buf)?;
                let payment_value: Option<u64> = BorshDeserialize::deserialize(buf)?;
                let change_value: u64 = BorshDeserialize::deserialize(buf)?;
                let accepted_daa_score: Option<u64> = BorshDeserialize::deserialize(buf)?;
                let utxo_entries: Vec<UtxoRecord> = BorshDeserialize::deserialize(buf)?;
                Ok(TransactionData::TransferIncoming {
                    fees,
                    aggregate_input_value,
                    aggregate_output_value,
                    transaction,
                    payment_value,
                    change_value,
                    accepted_daa_score,
                    utxo_entries,
                })
            }
            TransactionKind::TransferOutgoing => {
                let fees: u64 = BorshDeserialize::deserialize(buf)?;
                let aggregate_input_value: u64 = BorshDeserialize::deserialize(buf)?;
                let aggregate_output_value: u64 = BorshDeserialize::deserialize(buf)?;
                let transaction: Transaction = BorshDeserialize::deserialize(buf)?;
                let payment_value: Option<u64> = BorshDeserialize::deserialize(buf)?;
                let change_value: u64 = BorshDeserialize::deserialize(buf)?;
                let accepted_daa_score: Option<u64> = BorshDeserialize::deserialize(buf)?;
                let utxo_entries: Vec<UtxoRecord> = BorshDeserialize::deserialize(buf)?;
                Ok(TransactionData::TransferOutgoing {
                    fees,
                    aggregate_input_value,
                    aggregate_output_value,
                    transaction,
                    payment_value,
                    change_value,
                    accepted_daa_score,
                    utxo_entries,
                })
            }
            TransactionKind::Change => {
                let aggregate_input_value: u64 = BorshDeserialize::deserialize(buf)?;
                let aggregate_output_value: u64 = BorshDeserialize::deserialize(buf)?;
                let transaction: Transaction = BorshDeserialize::deserialize(buf)?;
                let payment_value: Option<u64> = BorshDeserialize::deserialize(buf)?;
                let change_value: u64 = BorshDeserialize::deserialize(buf)?;
                let accepted_daa_score: Option<u64> = BorshDeserialize::deserialize(buf)?;
                let utxo_entries: Vec<UtxoRecord> = BorshDeserialize::deserialize(buf)?;
                Ok(TransactionData::Change {
                    aggregate_input_value,
                    aggregate_output_value,
                    transaction,
                    payment_value,
                    change_value,
                    accepted_daa_score,
                    utxo_entries,
                })
            }
        }
    }
}
