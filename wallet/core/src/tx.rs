//use js_sys::Object;
use kaspa_addresses::Address;
use wasm_bindgen::prelude::*;
// pub use kaspa_consensus_core::wasm::MutableTransaction;

use kaspa_consensus_core::hashing::sighash::calc_schnorr_signature_hash;
use kaspa_consensus_core::hashing::sighash::SigHashReusedValues;
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::tx::ScriptVec;
use kaspa_consensus_core::tx::TransactionId;
// use kaspa_consensus_core::subnets::SubnetworkId;
use crate::utxo::*;
// use crate::tx;
use secp256k1::{rand, Secp256k1};

// ::{
//     // self,
//     // MutableTransaction,
//     // PopulatedTransaction,
//     // SignableTransaction,
//     // ScriptVec,
//     // TransactionId,
//     UtxoEntry,
// };
use kaspa_consensus_core::tx::{
    self,
    ScriptPublicKey,
    Transaction, // UtxoEntry,
    TransactionInput,
    TransactionOutpoint,
    TransactionOutput,
};
// use crate::tx;
// use crate::wasm::UtxoEntry;
use core::str::FromStr;
// use itertools::Itertools;
// use js_sys::Array;
// use secp256k1::Secp256k1;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::to_value;
// use std::iter::once;
use std::sync::{Arc, Mutex};
// use wasm_bindgen::prelude::*;
// use workflow_wasm::abi::ref_from_abi;

// use crate::sign::sign_with_multiple;
use kaspa_consensus_core::tx::SignableTransaction;
use kaspa_consensus_core::wasm::error::Error;
// use workflow_wasm::object::*;

// use crate::utxo::SelectionContext;
// use workflow_wasm::jsvalue::*;
// use kaspa_consensus_core::wasm::utxo::UtxoEntryList;

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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct MutableTransaction {
    //inner : Arc<tx::MutableTransaction<Transaction>>,
    tx: Arc<Mutex<Transaction>>,
    /// Partially filled UTXO entry data
    #[wasm_bindgen(getter_with_clone)]
    pub entries: UtxoEntryList,
    // Populated fee
    // #[wasm_bindgen(skip)]
    // pub calculated_fee: Option<u64>,
    // Populated mass
    // #[wasm_bindgen(skip)]
    // pub calculated_mass: Option<u64>,
}

#[wasm_bindgen]
impl MutableTransaction {
    #[wasm_bindgen(constructor)]
    pub fn constructor(tx: &Transaction, entries: &UtxoEntryList) -> Self {
        Self { tx: Arc::new(Mutex::new(tx.clone())), entries: entries.clone() }
        // Self { tx: Arc::new(Mutex::new(tx)), entries, calculated_fee: None, calculated_mass: None }
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

    // fn sign(js_value: JsValue) -> tx::MutableTransaction {

    //     // TODO - get signer
    //     // use signer.sign(self)

    // }

    // fn sign_with_key(js_value: JsValue) -> MutableTransaction {

    // }

    // pub fn as_signable(&self) -> SignableTransaction {
    //     todo!()
    // }
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

impl TryFrom<(tx::MutableTransaction<Transaction>, UtxoEntryList)> for MutableTransaction {
    type Error = Error;
    fn try_from(value: (tx::MutableTransaction<Transaction>, UtxoEntryList)) -> Result<Self, Self::Error> {
        Ok(Self {
            tx: Arc::new(Mutex::new(value.0.tx)),
            entries: value.1,
            // calculated_fee: value.calculated_fee,
            // calculated_mass: value.calculated_mass,
        })
    }
}

pub struct Destination {
    // outpoint: OutPoint,
}

pub struct TransactionOptions {}

#[allow(dead_code)] //TODO: remove me
pub struct Output {
    address: Address,
    amount: u64,
    utxo_entry: Option<Arc<UtxoEntry>>,
}

pub struct Outputs {
    pub outputs: Vec<Output>,
}

/// `VirtualTransaction` envelops a collection of multiple related `kaspa_wallet_coreMutableTransaction` instances.
#[derive(Clone)]
#[wasm_bindgen]
#[allow(dead_code)] //TODO: remove me
pub struct VirtualTransaction {
    transactions: Vec<MutableTransaction>,
    payload: Vec<u8>,
    // include_fees : bool,
}

impl VirtualTransaction {
    pub fn new(utxo_selection: SelectionContext, _outputs: &Outputs, payload: Vec<u8>) -> Self {
        let entries = &utxo_selection.selected_entries;

        let chunks = entries.chunks(80).collect::<Vec<&[UtxoEntryReference]>>();

        //let mut mutable: std::vec::Vec<T> = vec![];

        // ---------------------------------------------
        // TODO - get a set of destination addresses
        let secp = Secp256k1::new();
        let (_secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let script_pub_key = ScriptVec::from_slice(&public_key.serialize());
        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        // ---------------------------------------------

        let transactions = chunks
            .into_iter()
            .map(|chunk| {
                let utxos = chunk.iter().map(|reference| reference.utxo.clone()).collect::<Vec<Arc<UtxoEntry>>>();

                // let prev_tx_id = TransactionId::default();
                let inputs = utxos
                    .iter()
                    .enumerate()
                    .map(|(sequence, _utxo)| TransactionInput {
                        previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                        signature_script: vec![],
                        sequence: sequence as u64,
                        sig_op_count: 0,
                    })
                    .collect::<Vec<TransactionInput>>();

                let tx = Transaction::new(
                    0,
                    inputs,
                    // outputs.into(),
                    vec![
                        TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
                        TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
                    ],
                    1615462089000,
                    SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                    0,
                    vec![],
                );

                MutableTransaction { tx: Arc::new(Mutex::new(tx)), entries: (*entries).clone().try_into().unwrap() }
            })
            .collect();

        VirtualTransaction { transactions, payload }
    }
}
