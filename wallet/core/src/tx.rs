use crate::utxo::*;
use kaspa_addresses::Address;
use kaspa_consensus_core::hashing::sighash::calc_schnorr_signature_hash;
use kaspa_consensus_core::hashing::sighash::SigHashReusedValues;
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_core::hex::FromHex;
use kaspa_rpc_core::RpcTransactionOutput;
use kaspa_rpc_core::{RpcTransaction, RpcTransactionInput};
use kaspa_txscript::pay_to_address_script;
use serde::Deserializer;
use serde_wasm_bindgen::from_value;
use wasm_bindgen::convert::FromWasmAbi;
use wasm_bindgen::prelude::*;
use workflow_log::log_trace;

use core::str::FromStr;
use kaspa_consensus_core::tx::SignableTransaction;
use kaspa_consensus_core::tx::{self, Transaction, TransactionInput, TransactionOutput};
use kaspa_consensus_core::wasm::error::Error;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::to_value;
use std::sync::{Arc, Mutex};

pub fn script_hashes(mut mutable_tx: SignableTransaction) -> Result<Vec<kaspa_hashes::Hash>, Error> {
    let mut list = vec![];
    for i in 0..mutable_tx.tx.inputs.len() {
        mutable_tx.tx.inputs[i].sig_op_count = 1;
    }

    let mut reused_values = SigHashReusedValues::new();
    for i in 0..mutable_tx.tx.inputs.len() {
        let sig_hash = calc_schnorr_signature_hash(&mutable_tx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values);
        list.push(sig_hash);
    }
    Ok(list)
}

/// Represents a generic mutable transaction
#[derive(Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct MutableTransaction {
    tx: Arc<Mutex<Transaction>>,
    /// UTXO entry data
    #[wasm_bindgen(getter_with_clone)]
    pub entries: UtxoEntries,
}

#[wasm_bindgen]
impl MutableTransaction {
    #[wasm_bindgen(constructor)]
    pub fn new(tx: &Transaction, entries: &UtxoEntries) -> Self {
        Self { tx: Arc::new(Mutex::new(tx.clone())), entries: entries.clone() }
    }

    #[wasm_bindgen(js_name=toJSON)]
    pub fn to_json(&self) -> Result<String, JsError> {
        Ok(self.serialize(serde_json::value::Serializer)?.to_string())
    }

    #[wasm_bindgen(js_name=fromJSON)]
    pub fn from_json(json: &str) -> Result<MutableTransaction, JsError> {
        let mtx: Self = serde_json::from_value(serde_json::Value::from_str(json)?)?;
        Ok(mtx)
    }

    #[wasm_bindgen(js_name=getScriptHashes)]
    pub fn script_hashes(&self) -> Result<JsValue, JsError> {
        let hashes = script_hashes(self.clone().try_into()?)?;
        Ok(to_value(&hashes)?)
    }

    #[wasm_bindgen(js_name=setSignatures)]
    pub fn set_signatures(&self, signatures: js_sys::Array) -> Result<JsValue, JsError> {
        let signatures = signatures.iter().map(|s| s.as_string().unwrap()).collect::<Vec<_>>();

        {
            let mut locked = self.tx.lock();
            let tx = locked.as_mut().unwrap();

            if signatures.len() != tx.inputs.len() {
                return Err(Error::Custom("Signature counts dont match input counts".to_string()).into());
            }

            for (i, signature) in signatures.iter().enumerate().take(tx.inputs.len()) {
                tx.inputs[i].sig_op_count = 1;
                tx.inputs[i].signature_script = Vec::<u8>::from_hex(signature)?;
                //log_trace!("tx.inputs[i].signature_script: {:?}", tx.inputs[i].signature_script);
            }
        }

        let tx: RpcTransaction = (*self).clone().try_into()?;
        Ok(to_value(&tx)?)
    }

    #[wasm_bindgen(js_name=toRpcTransaction)]
    pub fn rpc_tx_request(&self) -> Result<JsValue, JsError> {
        let tx: RpcTransaction = (*self).clone().try_into()?;
        Ok(to_value(&tx)?)
    }
}

impl TryFrom<MutableTransaction> for tx::MutableTransaction<Transaction> {
    type Error = Error;
    fn try_from(mtx: MutableTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            tx: mtx.tx.lock()?.clone(),
            entries: mtx.entries.into(), //iter().map(|entry|entry.).collect(),
            calculated_fee: None,        //value.calculated_fee,
            calculated_mass: None,       //value.calculated_mass,
        })
    }
}

impl TryFrom<(tx::MutableTransaction<Transaction>, UtxoEntries)> for MutableTransaction {
    type Error = Error;
    fn try_from(value: (tx::MutableTransaction<Transaction>, UtxoEntries)) -> Result<Self, Self::Error> {
        Ok(Self {
            tx: Arc::new(Mutex::new(value.0.tx)),
            entries: value.1,
            // calculated_fee: value.calculated_fee,
            // calculated_mass: value.calculated_mass,
        })
    }
}

impl TryFrom<MutableTransaction> for RpcTransaction {
    type Error = Error;
    fn try_from(mtx: MutableTransaction) -> Result<Self, Self::Error> {
        let tx = tx::MutableTransaction::try_from(mtx)?.tx;

        let rpc_tx = RpcTransaction {
            version: tx.version,
            inputs: RpcTransactionInput::from_transaction_inputs(tx.inputs),
            outputs: RpcTransactionOutput::from_transaction_outputs(tx.outputs),
            lock_time: tx.lock_time,
            subnetwork_id: tx.subnetwork_id,
            gas: tx.gas,
            payload: tx.payload,
            verbose_data: None,
        };

        Ok(rpc_tx)
    }
}

pub struct Destination {
    // outpoint: OutPoint,
}

pub struct TransactionOptions {}

#[derive(Debug)]
#[wasm_bindgen(inspectable)]
#[allow(dead_code)] //TODO: remove me
pub struct Output {
    #[wasm_bindgen(getter_with_clone)]
    pub address: Address,
    pub amount: u64,
    utxo_entry: Option<Arc<UtxoEntry>>,
}

#[wasm_bindgen]
impl Output {
    #[wasm_bindgen(constructor)]
    pub fn new(address: Address, amount: u64, utxo_entry: Option<UtxoEntry>) -> Self {
        Self { address, amount, utxo_entry: utxo_entry.map(Arc::new) }
    }
}

impl<'de> Deserialize<'de> for Output {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(OutputVisitor)
    }
}

struct OutputVisitor;

impl<'de> serde::de::Visitor<'de> for OutputVisitor {
    type Value = Output;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "valid Output object.")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let key = map.next_key::<String>()?;
        let value = map.next_value::<u32>()?;

        if let Some(key) = &key {
            if key.eq("ptr") {
                return Ok(unsafe { Self::Value::from_abi(value) });
            }
        }
        Err(serde::de::Error::invalid_value(serde::de::Unexpected::Map, &self))
        //Err(serde::de::Error::invalid_value(serde::de::Unexpected::Str(&format!("Invalid address: {{{key:?}:{value:?}}}")), &self))
    }
}

#[derive(Debug)]
#[wasm_bindgen]
pub struct Outputs {
    #[wasm_bindgen(skip)]
    pub outputs: Vec<Output>,
}
#[wasm_bindgen]
impl Outputs {
    #[wasm_bindgen(constructor)]
    pub fn js_ctor(output_array: JsValue) -> crate::Result<Outputs> {
        let mut outputs = vec![];
        let iterator = js_sys::try_iter(&output_array)?.ok_or("need to pass iterable JS values!")?;
        for x in iterator {
            outputs.push(from_value(x?)?);
        }

        Ok(Self { outputs })
    }
}

/// `VirtualTransaction` envelops a collection of multiple related `kaspa_wallet_core::MutableTransaction` instances.
#[derive(Clone, Debug)]
#[wasm_bindgen]
#[allow(dead_code)] //TODO: remove me
pub struct VirtualTransaction {
    transactions: Vec<MutableTransaction>,
    payload: Vec<u8>,
    // include_fees : bool,
}

#[wasm_bindgen]
impl VirtualTransaction {
    #[wasm_bindgen(constructor)]
    pub fn new(
        utxo_selection: SelectionContext,
        outputs: Outputs,
        change_address: Address,
        payload: Vec<u8>,
    ) -> crate::Result<VirtualTransaction> {
        log_trace!("VirtualTransaction...");
        log_trace!("utxo_selection.transaction_amount: {:?}", utxo_selection.transaction_amount);
        log_trace!("outputs.outputs: {:?}", outputs.outputs);
        log_trace!("change_address: {change_address:?}");

        let entries = &utxo_selection.selected_entries;

        let chunks = entries.chunks(80).collect::<Vec<&[UtxoEntryReference]>>();

        //let mut mutable: std::vec::Vec<T> = vec![];

        // ---------------------------------------------
        // TODO - get a set of destination addresses
        //let secp = Secp256k1::new();
        //let (_secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        //let script_pub_key = ScriptVec::from_slice(&public_key.serialize());
        //let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        // ---------------------------------------------
        let mut final_inputs = vec![];
        let mut final_utxos = vec![];
        let mut final_amount = 0;
        let mut transactions = chunks
            .into_iter()
            .map(|chunk| {
                let utxos = chunk.iter().map(|reference| reference.utxo.clone()).collect::<Vec<Arc<UtxoEntry>>>();

                // let prev_tx_id = TransactionId::default();
                let mut amount = 0;
                let mut entries = vec![];

                let inputs = utxos
                    .iter()
                    .enumerate()
                    .map(|(sequence, utxo)| {
                        amount += utxo.utxo_entry.amount;
                        entries.push(utxo.as_ref().clone());
                        TransactionInput {
                            previous_outpoint: utxo.outpoint,
                            signature_script: vec![],
                            sequence: sequence as u64,
                            sig_op_count: 0,
                        }
                    })
                    .collect::<Vec<TransactionInput>>();

                let amount_after_fee = amount - 500; //TODO: calculate Fee

                let script_public_key = pay_to_address_script(&change_address);
                let tx = Transaction::new(
                    0,
                    inputs,
                    vec![TransactionOutput { value: amount_after_fee, script_public_key: script_public_key.clone() }],
                    0,
                    SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                    0,
                    vec![],
                );

                let transaction_id = tx.id();

                final_amount += amount_after_fee;
                final_utxos.push(UtxoEntry {
                    address: change_address.clone(),
                    outpoint: TransactionOutpoint { transaction_id, index: 0 },
                    utxo_entry: tx::UtxoEntry {
                        amount: amount_after_fee,
                        script_public_key,
                        block_daa_score: u64::MAX,
                        is_coinbase: false,
                    },
                });
                final_inputs.push(TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id, index: 0 },
                    signature_script: vec![],
                    sequence: final_inputs.len() as u64,
                    sig_op_count: 0,
                });

                MutableTransaction { tx: Arc::new(Mutex::new(tx)), entries: entries.into() }
            })
            .collect::<Vec<MutableTransaction>>();

        let fee = 500; //TODO: calculate Fee
        let amount_after_fee = final_amount - fee;

        let mut outputs_ = vec![];
        let mut total_amount = 0;
        for output in &outputs.outputs {
            total_amount += output.amount;
            outputs_.push(TransactionOutput { value: output.amount, script_public_key: pay_to_address_script(&output.address) });
        }

        if total_amount > amount_after_fee {
            return Err("total_amount({total_amount}) > amount_after_fee({amount_after_fee})".to_string().into());
        }

        let change = amount_after_fee - total_amount;
        let dust = 500;
        if change > dust {
            outputs_.push(TransactionOutput { value: change, script_public_key: pay_to_address_script(&change_address) });
        }

        let tx = Transaction::new(
            0,
            final_inputs,
            outputs_,
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            payload.clone(),
        );

        let mtx = MutableTransaction { tx: Arc::new(Mutex::new(tx)), entries: final_utxos.into() };
        transactions.push(mtx);

        log_trace!("transactions: {transactions:#?}");

        Ok(VirtualTransaction { transactions, payload })
    }
}

#[wasm_bindgen(js_name=createTransaction)]
pub fn create_transaction(
    utxo_selection: SelectionContext,
    outputs: Outputs,
    change_address: Address,
    priority_fee: Option<u32>,
    payload: Option<Vec<u8>>,
) -> crate::Result<MutableTransaction> {
    let entries = &utxo_selection.selected_entries;

    let utxos = entries.iter().map(|reference| reference.utxo.clone()).collect::<Vec<Arc<UtxoEntry>>>();

    // let prev_tx_id = TransactionId::default();
    let mut amount = 0;
    let mut entries = vec![];

    let inputs = utxos
        .iter()
        .enumerate()
        .map(|(sequence, utxo)| {
            amount += utxo.utxo_entry.amount;
            entries.push(utxo.as_ref().clone());
            TransactionInput { previous_outpoint: utxo.outpoint, signature_script: vec![], sequence: sequence as u64, sig_op_count: 0 }
        })
        .collect::<Vec<TransactionInput>>();

    let fee = 2036 + priority_fee.unwrap_or(0) as u64; //TODO: calculate Fee
    if fee > amount {
        return Err(format!("fee({fee}) > amount({amount})").into());
    }
    let amount_after_fee = amount - fee;

    let mut outputs_ = vec![];
    let mut total_amount = 0;
    for output in &outputs.outputs {
        total_amount += output.amount;
        outputs_.push(TransactionOutput { value: output.amount, script_public_key: pay_to_address_script(&output.address) });
    }

    if total_amount > amount_after_fee {
        return Err(format!("total_amount({total_amount}) > amount_after_fee({amount_after_fee})").into());
    }

    let change = amount_after_fee - total_amount;
    let dust = 500;
    if change > dust {
        outputs_.push(TransactionOutput { value: change, script_public_key: pay_to_address_script(&change_address) });
    }

    let tx = Transaction::new(
        0,
        inputs,
        outputs_,
        0,
        SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        0,
        payload.unwrap_or(vec![]),
    );

    let mtx = MutableTransaction { tx: Arc::new(Mutex::new(tx)), entries: entries.into() };

    Ok(mtx)
}
