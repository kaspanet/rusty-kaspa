//! This module implements the primitives for external transaction signing.

use crate::error::Error;
use crate::imports::*;
use crate::result::Result;
use crate::{
    Transaction, TransactionInput, TransactionInputInner, TransactionOutpoint, TransactionOutpointInner, TransactionOutput, UtxoEntry,
    UtxoEntryId, UtxoEntryReference,
};
use ahash::AHashMap;
use cctx::VerifiableTransaction;
use kaspa_addresses::Address;
use kaspa_consensus_core::subnets::SubnetworkId;
use workflow_wasm::serde::{from_value, to_value};

pub type SignedTransactionIndexType = u32;

pub struct Options {
    pub include_utxo: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableUtxoEntry {
    pub address: Option<Address>,
    pub amount: u64,
    pub script_public_key: ScriptPublicKey,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl AsRef<SerializableUtxoEntry> for SerializableUtxoEntry {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl From<&UtxoEntryReference> for SerializableUtxoEntry {
    fn from(utxo: &UtxoEntryReference) -> Self {
        let utxo = utxo.utxo.as_ref();
        Self {
            address: utxo.address.clone(),
            amount: utxo.amount,
            script_public_key: utxo.script_public_key.clone(),
            block_daa_score: utxo.block_daa_score,
            is_coinbase: utxo.is_coinbase,
        }
    }
}

impl From<&cctx::UtxoEntry> for SerializableUtxoEntry {
    fn from(utxo: &cctx::UtxoEntry) -> Self {
        Self {
            address: None,
            amount: utxo.amount,
            script_public_key: utxo.script_public_key.clone(),
            block_daa_score: utxo.block_daa_score,
            is_coinbase: utxo.is_coinbase,
        }
    }
}

impl TryFrom<&SerializableUtxoEntry> for cctx::UtxoEntry {
    type Error = crate::error::Error;
    fn try_from(utxo: &SerializableUtxoEntry) -> Result<Self> {
        Ok(Self {
            amount: utxo.amount,
            script_public_key: utxo.script_public_key.clone(),
            block_daa_score: utxo.block_daa_score,
            is_coinbase: utxo.is_coinbase,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransactionInput {
    pub transaction_id: TransactionId,
    pub index: SignedTransactionIndexType,
    pub sequence: u64,
    pub sig_op_count: u8,
    #[serde(with = "hex::serde")]
    pub signature_script: Vec<u8>,
    pub utxo: SerializableUtxoEntry,
}

impl SerializableTransactionInput {
    pub fn new(input: &cctx::TransactionInput, utxo: &cctx::UtxoEntry) -> Self {
        let utxo = SerializableUtxoEntry::from(utxo);

        Self {
            transaction_id: input.previous_outpoint.transaction_id,
            index: input.previous_outpoint.index,
            signature_script: input.signature_script.clone(),
            sequence: input.sequence,
            sig_op_count: input.sig_op_count,
            utxo: utxo.clone(),
        }
    }
}

impl TryFrom<&SerializableTransactionInput> for UtxoEntryReference {
    type Error = Error;
    fn try_from(input: &SerializableTransactionInput) -> Result<Self> {
        let outpoint = TransactionOutpoint::new(input.transaction_id, input.index);

        let utxo = UtxoEntry {
            outpoint,
            address: input.utxo.address.clone(),
            amount: input.utxo.amount,
            script_public_key: input.utxo.script_public_key.clone(),
            block_daa_score: input.utxo.block_daa_score,
            is_coinbase: input.utxo.is_coinbase,
        };

        Ok(Self { utxo: Arc::new(utxo) })
    }
}

impl TryFrom<SerializableTransactionInput> for cctx::TransactionInput {
    type Error = Error;
    fn try_from(signable_input: SerializableTransactionInput) -> Result<Self> {
        Ok(Self {
            previous_outpoint: cctx::TransactionOutpoint {
                transaction_id: signable_input.transaction_id,
                index: signable_input.index,
            },
            signature_script: signable_input.signature_script,
            sequence: signable_input.sequence,
            sig_op_count: signable_input.sig_op_count,
        })
    }
}

impl TryFrom<&SerializableTransactionInput> for TransactionInput {
    type Error = Error;
    fn try_from(signable_input: &SerializableTransactionInput) -> Result<Self> {
        let utxo = UtxoEntryReference::try_from(signable_input)?;

        let previous_outpoint = TransactionOutpoint::new(signable_input.transaction_id, signable_input.index);
        let inner = TransactionInputInner {
            previous_outpoint,
            signature_script: signable_input.signature_script.clone(),
            sequence: signable_input.sequence,
            sig_op_count: signable_input.sig_op_count,
            utxo: Some(utxo),
        };

        Ok(TransactionInput::new_with_inner(inner))
    }
}

impl TryFrom<&TransactionInput> for SerializableTransactionInput {
    type Error = Error;
    fn try_from(input: &TransactionInput) -> Result<Self> {
        let inner = input.inner();
        let utxo = inner.utxo.as_ref().ok_or(Error::MissingUtxoEntry)?;
        let utxo = SerializableUtxoEntry::from(utxo);
        Ok(Self {
            transaction_id: inner.previous_outpoint.transaction_id(),
            index: inner.previous_outpoint.index(),
            signature_script: inner.signature_script.clone(),
            sequence: inner.sequence,
            sig_op_count: inner.sig_op_count,
            utxo,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransactionOutput {
    pub value: u64,
    pub script_public_key: ScriptPublicKey,
}

impl From<cctx::TransactionOutput> for SerializableTransactionOutput {
    fn from(output: cctx::TransactionOutput) -> Self {
        Self { value: output.value, script_public_key: output.script_public_key }
    }
}

impl From<&cctx::TransactionOutput> for SerializableTransactionOutput {
    fn from(output: &cctx::TransactionOutput) -> Self {
        Self { value: output.value, script_public_key: output.script_public_key.clone() }
    }
}

impl TryFrom<SerializableTransactionOutput> for cctx::TransactionOutput {
    type Error = Error;
    fn try_from(output: SerializableTransactionOutput) -> Result<Self> {
        Ok(Self { value: output.value, script_public_key: output.script_public_key })
    }
}

impl TryFrom<&SerializableTransactionOutput> for TransactionOutput {
    type Error = Error;
    fn try_from(output: &SerializableTransactionOutput) -> Result<Self> {
        Ok(TransactionOutput::new(output.value, output.script_public_key.clone()))
    }
}

impl TryFrom<&TransactionOutput> for SerializableTransactionOutput {
    type Error = Error;
    fn try_from(output: &TransactionOutput) -> Result<Self> {
        let inner = output.inner();
        Ok(Self { value: inner.value, script_public_key: inner.script_public_key.clone() })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransaction {
    // pub version: u32,
    pub id: TransactionId,
    pub version: u16,
    pub inputs: Vec<SerializableTransactionInput>,
    pub outputs: Vec<SerializableTransactionOutput>,
    pub lock_time: u64,
    pub gas: u64,
    pub subnetwork_id: SubnetworkId,
    #[serde(with = "hex::serde")]
    pub payload: Vec<u8>,
}

impl SerializableTransaction {
    pub fn serialize_to_object(&self) -> Result<JsValue> {
        Ok(to_value(self)?)
    }

    pub fn deserialize_from_object(object: JsValue) -> Result<Self> {
        Ok(from_value(object)?)
    }

    pub fn serialize_to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn deserialize_from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn from_signable_transaction(tx: &cctx::SignableTransaction) -> Result<Self> {
        let verifiable_tx = tx.as_verifiable();
        let mut inputs = vec![];
        let transaction = tx.as_ref();
        for index in 0..transaction.inputs.len() {
            let (input, utxo) = verifiable_tx.populated_input(index);
            let input = SerializableTransactionInput::new(input, utxo);
            inputs.push(input);
        }

        let outputs = transaction.outputs.clone();

        Ok(Self {
            // version: 2,
            inputs,
            outputs: outputs.into_iter().map(Into::into).collect(),
            version: transaction.version,
            lock_time: transaction.lock_time,
            subnetwork_id: transaction.subnetwork_id.clone(),
            gas: transaction.gas,
            payload: transaction.payload.clone(),
            id: transaction.id(),
        })
    }

    pub fn from_client_transaction(transaction: &Transaction) -> Result<Self> {
        let inner = transaction.inner();

        let inputs = inner.inputs.iter().map(TryFrom::try_from).collect::<Result<Vec<SerializableTransactionInput>>>()?;
        let outputs = inner.outputs.iter().map(TryFrom::try_from).collect::<Result<Vec<SerializableTransactionOutput>>>()?;

        Ok(Self {
            inputs,
            outputs,
            version: inner.version,
            lock_time: inner.lock_time,
            subnetwork_id: inner.subnetwork_id.clone(),
            gas: inner.gas,
            payload: inner.payload.clone(),
            id: inner.id,
        })
    }

    pub fn from_cctx_transaction(transaction: &cctx::Transaction, utxos: &AHashMap<UtxoEntryId, UtxoEntryReference>) -> Result<Self> {
        let inputs = transaction
            .inputs
            .iter()
            .map(|input| {
                let id = TransactionOutpointInner::new(input.previous_outpoint.transaction_id, input.previous_outpoint.index);
                let utxo = utxos.get(&id).ok_or(Error::MissingUtxoEntry)?;
                let utxo = cctx::UtxoEntry::from(utxo);
                let input = SerializableTransactionInput::new(input, &utxo);
                Ok(input)
            })
            .collect::<Result<Vec<SerializableTransactionInput>>>()?;

        let outputs = transaction.outputs.iter().map(Into::into).collect::<Vec<SerializableTransactionOutput>>();

        Ok(Self {
            id: transaction.id(),
            version: transaction.version,
            inputs,
            outputs,
            lock_time: transaction.lock_time,
            subnetwork_id: transaction.subnetwork_id.clone(),
            gas: transaction.gas,
            payload: transaction.payload.clone(),
        })
    }
}

impl TryFrom<SerializableTransaction> for cctx::SignableTransaction {
    type Error = Error;
    fn try_from(serializable: SerializableTransaction) -> Result<Self> {
        let mut entries = vec![];
        let mut inputs = vec![];
        for input in serializable.inputs {
            entries.push(input.utxo.as_ref().try_into()?);
            inputs.push(input.try_into()?);
        }

        let outputs = serializable.outputs.into_iter().map(TryInto::try_into).collect::<Result<Vec<_>>>()?;

        let tx = cctx::Transaction::new(
            serializable.version,
            inputs,
            outputs,
            serializable.lock_time,
            serializable.subnetwork_id,
            serializable.gas,
            serializable.payload,
        );

        Ok(Self::with_entries(tx, entries))
    }
}

impl TryFrom<SerializableTransaction> for Transaction {
    type Error = Error;
    fn try_from(tx: SerializableTransaction) -> Result<Self> {
        let id = tx.id;
        let inputs: Vec<TransactionInput> = tx.inputs.iter().map(TryInto::try_into).collect::<Result<Vec<_>>>()?;
        let outputs: Vec<TransactionOutput> = tx.outputs.iter().map(TryInto::try_into).collect::<Result<Vec<_>>>()?;

        Transaction::new(Some(id), tx.version, inputs, outputs, tx.lock_time, tx.subnetwork_id, tx.gas, tx.payload)
    }
}
