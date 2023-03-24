pub mod convert;
pub mod error;
pub mod keypair;
pub mod signer;

// use crate::config::params::MAINNET_PARAMS;
// use consensus_core::sign::sign;
use consensus_core::subnets::SubnetworkId;
use consensus_core::tx::{
    self,
    // MutableTransaction,
    // PopulatedTransaction,
    // SignableTransaction,
    ScriptVec,
    TransactionId,
    UtxoEntry,
};
use consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use core::str::FromStr;
use itertools::Itertools;
use js_sys::Array;
use secp256k1::Secp256k1;
use std::iter::once;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use workflow_wasm::abi::ref_from_abi;

use crate::sign::sign_with_multiple;
use crate::tx::SignableTransaction;

//use signer::Signer as _Signer;

// impl Signer for Generator {}

#[derive(Clone, Debug)]
#[wasm_bindgen]
pub struct UtxoEntryList(Arc<Vec<UtxoEntry>>);

#[wasm_bindgen]
impl UtxoEntryList {
    #[wasm_bindgen(constructor)]
    pub fn js_ctor(js_value: JsValue) -> Result<UtxoEntryList, JsError> {
        Ok(js_value.try_into()?)
    }
    #[wasm_bindgen(getter = items)]
    pub fn get_items_as_js_array(&self) -> JsValue {
        let items = self.0.as_ref().clone().into_iter().map(<UtxoEntry as Into<JsValue>>::into);
        Array::from_iter(items).into()
    }

    #[wasm_bindgen(setter = items)]
    pub fn set_items_from_js_array(&mut self, js_value: &JsValue) {
        let items = Array::from(js_value)
            .iter()
            .map(|js_value| ref_from_abi!(UtxoEntry, &js_value).unwrap_or_else(|err| panic!("invalid UTXOEntry: {err}")))
            .collect::<Vec<_>>();
        self.0 = Arc::new(items);
    }
}

impl From<UtxoEntryList> for Vec<Option<UtxoEntry>> {
    fn from(value: UtxoEntryList) -> Self {
        value.0.as_ref().iter().map(|entry| Some(entry.clone())).collect_vec()
    }
}

impl TryFrom<Vec<Option<UtxoEntry>>> for UtxoEntryList {
    type Error = error::Error;
    fn try_from(value: Vec<Option<UtxoEntry>>) -> Result<Self, Self::Error> {
        let mut list = vec![];
        for entry in value.into_iter() {
            list.push(entry.ok_or(error::Error::Custom("Unable to cast `Vec<Option<UtxoEntry>>` into `UtxoEntryList`.".to_string()))?);
        }

        Ok(Self(Arc::new(list)))
    }
}

#[derive(Clone, Debug)]
#[wasm_bindgen]
pub struct MutableTransaction {
    //inner : Arc<tx::MutableTransaction<Transaction>>,
    tx: Arc<Mutex<tx::Transaction>>,
    /// Partially filled UTXO entry data
    #[wasm_bindgen(getter_with_clone)]
    pub entries: UtxoEntryList, // Vec<Option<UtxoEntry>>,
    /// Populated fee
    pub calculated_fee: Option<u64>,
    /// Populated mass
    pub calculated_mass: Option<u64>,
}

#[wasm_bindgen]
impl MutableTransaction {
    #[wasm_bindgen(constructor)]
    pub fn constructor(tx: tx::Transaction, entries: UtxoEntryList) -> Self {
        Self { tx: Arc::new(Mutex::new(tx)), entries, calculated_fee: None, calculated_mass: None }
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

impl From<MutableTransaction> for tx::MutableTransaction<Transaction> {
    fn from(value: MutableTransaction) -> Self {
        Self {
            tx: value.tx.lock().unwrap().clone(),
            entries: value.entries.into(),
            calculated_fee: value.calculated_fee,
            calculated_mass: value.calculated_mass,
        }
    }
}

impl TryFrom<tx::MutableTransaction<Transaction>> for MutableTransaction {
    type Error = error::Error;
    fn try_from(value: tx::MutableTransaction<Transaction>) -> Result<Self, Self::Error> {
        Ok(Self {
            tx: Arc::new(Mutex::new(value.tx)),
            entries: value.entries.try_into()?,
            calculated_fee: value.calculated_fee,
            calculated_mass: value.calculated_mass,
        })
    }
}

// #[derive(Clone, Debug)]
// #[wasm_bindgen]
// pub struct XSignableTransaction {}

// type txs = tx::SignableTransaxtion;

// pub fn _sign(tx : &MutableTransaction, entries : &UtxoEntryList, signer : dyn Signer) -> MutableTransaction

// pub fn _sign(mut signable_tx: SignableTransaction, privkey: [u8; 32]) -> SignableTransaction {
//     todo!()
// }

// test code taken from consensus/src/processes/transaction_validator/transaction_validator_populated.rs
#[allow(dead_code)]
fn test_sign() {
    // let params = MAINNET_PARAMS.clone();
    // let tv = TransactionValidator::new(
    //     params.max_tx_inputs,
    //     params.max_tx_outputs,
    //     params.max_signature_script_len,
    //     params.max_script_public_key_len,
    //     params.ghostdag_k,
    //     params.coinbase_payload_script_public_key_max_len,
    //     params.coinbase_maturity,
    // );

    let secp = Secp256k1::new();
    let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
    let (public_key, _) = public_key.x_only_public_key();
    let script_pub_key = once(0x20).chain(public_key.serialize().into_iter()).chain(once(0xac)).collect_vec();
    let script_pub_key = ScriptVec::from_slice(&script_pub_key);

    let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
    let unsigned_tx = Transaction::new(
        0,
        vec![
            TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 0,
            },
            TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                signature_script: vec![],
                sequence: 1,
                sig_op_count: 0,
            },
            TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 2 },
                signature_script: vec![],
                sequence: 2,
                sig_op_count: 0,
            },
        ],
        vec![
            TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
            TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
        ],
        1615462089000,
        SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        0,
        vec![],
    );

    let entries = vec![
        UtxoEntry {
            amount: 100,
            script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
            block_daa_score: 0,
            is_coinbase: false,
        },
        UtxoEntry {
            amount: 200,
            script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
            block_daa_score: 0,
            is_coinbase: false,
        },
        UtxoEntry { amount: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key), block_daa_score: 0, is_coinbase: false },
    ];
    let signed_tx = sign_with_multiple(tx::MutableTransaction::with_entries(unsigned_tx, entries), vec![secret_key.secret_bytes()]);
    let _populated_tx = signed_tx.as_verifiable();
    // assert_eq!(tv.check_scripts(&populated_tx), Ok(()));
    // assert_eq!(TransactionValidator::check_sig_op_counts(&populated_tx), Ok(()));
}
