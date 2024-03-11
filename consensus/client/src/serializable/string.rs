//! Primitives for external transaction signing

use crate::imports::*;
use crate::result::Result;
use cctx::VerifiableTransaction;
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::tx as cctx;
use kaspa_consensus_core::tx::TransactionInput;
use workflow_wasm::serde::{from_value, to_value};

pub type SignedTransactionIndexType = u32;

pub struct Options {
    pub include_utxo: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableUtxoEntry {
    pub amount: String,
    pub script_public_key: ScriptPublicKey,
    pub block_daa_score: String,
    pub is_coinbase: bool,
}

impl AsRef<SerializableUtxoEntry> for SerializableUtxoEntry {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl From<&cctx::UtxoEntry> for SerializableUtxoEntry {
    fn from(utxo: &cctx::UtxoEntry) -> Self {
        Self {
            amount: utxo.amount.to_string(),
            script_public_key: utxo.script_public_key.clone(),
            block_daa_score: utxo.block_daa_score.to_string(),
            is_coinbase: utxo.is_coinbase,
        }
    }
}

impl TryFrom<&SerializableUtxoEntry> for cctx::UtxoEntry {
    type Error = crate::error::Error;
    fn try_from(utxo: &SerializableUtxoEntry) -> Result<Self> {
        Ok(Self {
            amount: utxo.amount.parse()?,
            script_public_key: utxo.script_public_key.clone(),
            block_daa_score: utxo.block_daa_score.parse()?,
            is_coinbase: utxo.is_coinbase,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransactionInput {
    pub transaction_id: TransactionId,
    pub index: SignedTransactionIndexType,
    pub sequence: String,
    pub sig_op_count: u8,
    #[serde(with = "hex::serde")]
    pub signature_script: Vec<u8>,
    pub utxo: SerializableUtxoEntry,
}

impl SerializableTransactionInput {
    pub fn new(input: &TransactionInput, utxo: &cctx::UtxoEntry) -> Self {
        let utxo = SerializableUtxoEntry::from(utxo);

        Self {
            transaction_id: input.previous_outpoint.transaction_id,
            index: input.previous_outpoint.index,
            signature_script: input.signature_script.clone(),
            sequence: input.sequence.to_string(),
            sig_op_count: input.sig_op_count,
            utxo: utxo.clone(),
        }
    }
}

impl TryFrom<SerializableTransactionInput> for kaspa_consensus_core::tx::TransactionInput {
    type Error = Error;
    fn try_from(signable_input: SerializableTransactionInput) -> Result<Self> {
        Ok(Self {
            previous_outpoint: cctx::TransactionOutpoint {
                transaction_id: signable_input.transaction_id,
                index: signable_input.index,
            },
            signature_script: signable_input.signature_script,
            sequence: signable_input.sequence.parse()?,
            sig_op_count: signable_input.sig_op_count,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransactionOutput {
    pub value: String,
    pub script_public_key: ScriptPublicKey,
}

impl From<cctx::TransactionOutput> for SerializableTransactionOutput {
    fn from(output: cctx::TransactionOutput) -> Self {
        Self { value: output.value.to_string(), script_public_key: output.script_public_key }
    }
}

impl TryFrom<SerializableTransactionOutput> for cctx::TransactionOutput {
    type Error = Error;
    fn try_from(output: SerializableTransactionOutput) -> Result<Self> {
        Ok(Self { value: output.value.parse()?, script_public_key: output.script_public_key })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransaction {
    pub version: u32,
    pub inputs: Vec<SerializableTransactionInput>,
    pub outputs: Vec<SerializableTransactionOutput>,
    pub tx_id: TransactionId,
    pub tx_version: u16,
    pub subnetwork_id: SubnetworkId,
    pub lock_time: String,
    pub gas: String,
    #[serde(with = "hex::serde")]
    pub payload: Vec<u8>,
}

impl SerializableTransaction {
    pub fn serialize_to_object(&self) -> JsValue {
        to_value(self).unwrap()
    }

    pub fn deserialize_from_object(object: JsValue) -> Result<Self> {
        Ok(from_value(object)?)
    }

    pub fn serialize_to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
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
            version: 2,
            inputs,
            outputs: outputs.into_iter().map(Into::into).collect(),
            tx_version: transaction.version,
            lock_time: transaction.lock_time.to_string(),
            subnetwork_id: transaction.subnetwork_id.clone(),
            gas: transaction.gas.to_string(),
            payload: transaction.payload.clone(),
            tx_id: transaction.id(),
        })
    }
}

impl TryFrom<SerializableTransaction> for cctx::SignableTransaction {
    type Error = Error;
    fn try_from(signable: SerializableTransaction) -> Result<Self> {
        let mut entries = vec![];
        let mut inputs = vec![];
        for input in signable.inputs {
            entries.push(input.utxo.as_ref().try_into()?);
            inputs.push(input.try_into()?);
        }

        let outputs = signable.outputs.into_iter().map(TryInto::try_into).collect::<Result<Vec<_>>>()?;

        let tx = cctx::Transaction::new(
            signable.tx_version,
            inputs,
            outputs,
            signable.lock_time.parse()?,
            signable.subnetwork_id,
            signable.gas.parse()?,
            signable.payload,
        );

        Ok(Self::with_entries(tx, entries))
    }
}
