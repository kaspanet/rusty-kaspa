use crate::imports::*;
use crate::result::Result;
use crate::runtime::{Account, Balance};
use crate::tx::{TransactionOutpoint, TransactionOutpointInner};
use itertools::Itertools;
use kaspa_rpc_core::RpcUtxosByAddressesEntry;
use std::cmp::Ordering;
use workflow_wasm::abi::{ref_from_abi, TryFromJsValue};

use super::UtxoContext;

// thresholds for 1 BPS network
pub const MATURITY_PERIOD_COINBASE_TRANSACTION: u64 = 128;
pub const MATURITY_PERIOD_USER_TRANSACTION: u64 = 16;

pub type UtxoEntryId = TransactionOutpointInner;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct UtxoEntry {
    #[wasm_bindgen(getter_with_clone)]
    pub address: Option<Address>,
    #[wasm_bindgen(getter_with_clone)]
    pub outpoint: TransactionOutpoint,
    #[wasm_bindgen(js_name=entry, getter_with_clone)]
    pub entry: cctx::UtxoEntry,
}

impl UtxoEntry {
    #[inline(always)]
    pub fn amount(&self) -> u64 {
        self.entry.amount
    }
    #[inline(always)]
    pub fn block_daa_score(&self) -> u64 {
        self.entry.block_daa_score
    }

    #[inline(always)]
    pub fn is_coinbase(&self) -> bool {
        self.entry.is_coinbase
    }

    #[inline(always)]
    pub fn is_mature(&self, current_daa_score: u64) -> bool {
        if self.is_coinbase() {
            self.block_daa_score() + MATURITY_PERIOD_COINBASE_TRANSACTION < current_daa_score
        } else {
            self.block_daa_score() + MATURITY_PERIOD_USER_TRANSACTION < current_daa_score
        }
    }

    pub fn balance(&self, current_daa_score: u64) -> Balance {
        if self.is_mature(current_daa_score) {
            Balance::new(self.amount(), 0)
        } else {
            Balance::new(0, self.amount())
        }
    }
}

impl From<RpcUtxosByAddressesEntry> for UtxoEntry {
    fn from(entry: RpcUtxosByAddressesEntry) -> UtxoEntry {
        UtxoEntry { address: entry.address, outpoint: entry.outpoint.try_into().unwrap(), entry: entry.utxo_entry }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TryFromJsValue)]
#[wasm_bindgen(inspectable)]
pub struct UtxoEntryReference {
    #[wasm_bindgen(skip)]
    pub utxo: Arc<UtxoEntry>,
}

impl UtxoEntryReference {
    pub fn is_mature(&self, current_daa_score: u64) -> bool {
        self.utxo.is_mature(current_daa_score)
    }
}

#[wasm_bindgen]
impl UtxoEntryReference {
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> UtxoEntry {
        self.as_ref().clone()
    }

    #[wasm_bindgen(js_name = "getTransactionId")]
    pub fn transaction_id_as_string(&self) -> String {
        self.utxo.outpoint.get_transaction_id_as_string()
    }

    #[wasm_bindgen(js_name = "getId")]
    pub fn id_string(&self) -> String {
        self.utxo.outpoint.id_string()
    }

    #[inline(always)]
    pub fn amount(&self) -> u64 {
        self.utxo.amount()
    }

    #[inline(always)]
    #[wasm_bindgen(js_name = "isCoinbase")]
    pub fn is_coinbase(&self) -> bool {
        self.utxo.entry.is_coinbase
    }

    #[inline(always)]
    #[wasm_bindgen(js_name = "blockDaaScore")]
    pub fn block_daa_score(&self) -> u64 {
        self.utxo.entry.block_daa_score
    }
}

impl UtxoEntryReference {
    pub fn id(&self) -> UtxoEntryId {
        self.utxo.outpoint.inner().clone()
    }

    pub fn transaction_id(&self) -> TransactionId {
        self.utxo.outpoint.transaction_id()
    }
}

impl AsRef<UtxoEntry> for UtxoEntryReference {
    fn as_ref(&self) -> &UtxoEntry {
        &self.utxo
    }
}

impl From<UtxoEntryReference> for UtxoEntry {
    fn from(value: UtxoEntryReference) -> Self {
        (*value.utxo).clone()
    }
}

impl From<RpcUtxosByAddressesEntry> for UtxoEntryReference {
    fn from(entry: RpcUtxosByAddressesEntry) -> Self {
        Self { utxo: Arc::new(entry.into()) }
    }
}

impl From<UtxoEntry> for UtxoEntryReference {
    fn from(entry: UtxoEntry) -> Self {
        Self { utxo: Arc::new(entry) }
    }
}

impl Eq for UtxoEntryReference {}

impl PartialEq for UtxoEntryReference {
    fn eq(&self, other: &Self) -> bool {
        self.amount() == other.amount()
    }
}

impl PartialOrd for UtxoEntryReference {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.amount().cmp(&other.amount()))
    }
}

impl Ord for UtxoEntryReference {
    fn cmp(&self, other: &Self) -> Ordering {
        self.amount().cmp(&other.amount())
    }
}

#[derive(Clone)]
pub struct PendingUtxoEntryReference {
    pub entry: UtxoEntryReference,
    pub utxo_context: UtxoContext,
}

impl PendingUtxoEntryReference {
    pub fn new(entry: UtxoEntryReference, utxo_context: UtxoContext) -> Self {
        Self { entry, utxo_context }
    }

    pub fn id(&self) -> UtxoEntryId {
        self.entry.id()
    }

    #[inline(always)]
    pub fn is_mature(&self, current_daa_score: u64) -> bool {
        self.entry.is_mature(current_daa_score)
    }
}

impl From<(&Arc<Account>, UtxoEntryReference)> for PendingUtxoEntryReference {
    fn from((account, entry): (&Arc<Account>, UtxoEntryReference)) -> Self {
        Self { entry, utxo_context: (**account.utxo_context()).clone() }
    }
}

impl From<PendingUtxoEntryReference> for UtxoEntryReference {
    fn from(entry: PendingUtxoEntryReference) -> Self {
        entry.entry
    }
}

// ---

#[derive(Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct UtxoEntries(Arc<Vec<UtxoEntryReference>>);

#[wasm_bindgen]
impl UtxoEntries {
    #[wasm_bindgen(constructor)]
    pub fn js_ctor(js_value: JsValue) -> Result<UtxoEntries> {
        js_value.try_into()
    }
    #[wasm_bindgen(getter = items)]
    pub fn get_items_as_js_array(&self) -> JsValue {
        let items = self.0.as_ref().clone().into_iter().map(<UtxoEntryReference as Into<JsValue>>::into);
        Array::from_iter(items).into()
    }

    #[wasm_bindgen(setter = items)]
    pub fn set_items_from_js_array(&mut self, js_value: &JsValue) {
        let items = Array::from(js_value)
            .iter()
            .map(|js_value| {
                ref_from_abi!(UtxoEntryReference, &js_value).unwrap_or_else(|err| panic!("invalid UtxoEntryReference: {err}"))
            })
            .collect::<Vec<_>>();
        self.0 = Arc::new(items);
    }
}
impl UtxoEntries {
    pub fn items(&self) -> Arc<Vec<UtxoEntryReference>> {
        self.0.clone()
    }
}

impl From<UtxoEntries> for Vec<Option<UtxoEntry>> {
    fn from(value: UtxoEntries) -> Self {
        value.0.as_ref().iter().map(|entry| Some(entry.as_ref().clone())).collect_vec()
    }
}

impl From<Vec<UtxoEntry>> for UtxoEntries {
    fn from(items: Vec<UtxoEntry>) -> Self {
        Self(Arc::new(items.into_iter().map(UtxoEntryReference::from).collect::<_>()))
    }
}

impl From<UtxoEntries> for Vec<Option<cctx::UtxoEntry>> {
    fn from(value: UtxoEntries) -> Self {
        value.0.as_ref().iter().map(|entry| Some(entry.utxo.entry.clone())).collect_vec()
    }
}

impl TryFrom<Vec<Option<UtxoEntry>>> for UtxoEntries {
    type Error = Error;
    fn try_from(value: Vec<Option<UtxoEntry>>) -> std::result::Result<Self, Self::Error> {
        let mut list = vec![];
        for entry in value.into_iter() {
            list.push(entry.ok_or(Error::Custom("Unable to cast `Vec<Option<UtxoEntry>>` into `UtxoEntries`.".to_string()))?.into());
        }

        Ok(Self(Arc::new(list)))
    }
}

impl TryFrom<Vec<UtxoEntryReference>> for UtxoEntries {
    type Error = Error;
    fn try_from(list: Vec<UtxoEntryReference>) -> std::result::Result<Self, Self::Error> {
        Ok(Self(Arc::new(list)))
    }
}

impl TryFrom<JsValue> for UtxoEntries {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        if !js_value.is_array() {
            return Err("UtxoEntries must be an array".into());
        }

        let mut list = vec![];
        for entry in Array::from(&js_value).iter() {
            list.push(match ref_from_abi!(UtxoEntryReference, &entry) {
                Ok(value) => value,
                Err(err) => {
                    if !entry.is_object() {
                        panic!("invalid UTXOEntry: {err}")
                    }
                    //log_trace!("entry: {:?}", entry);
                    let object = Object::from(entry);
                    let amount = object.get_u64("amount")?;
                    let script_public_key = ScriptPublicKey::try_from_jsvalue(
                        object.get("scriptPublicKey").map_err(|_| Error::Custom("missing `scriptPublicKey` property".into()))?,
                    )?;
                    let block_daa_score = object.get_u64("blockDaaScore")?;
                    let is_coinbase = object.get_bool("isCoinbase")?;
                    let address: Address = object.get_string("address")?.try_into()?;
                    let outpoint: TransactionOutpoint = object.get("outpoint")?.try_into()?;
                    UtxoEntry {
                        address: address.into(),
                        outpoint,
                        entry: cctx::UtxoEntry { amount, script_public_key, block_daa_score, is_coinbase },
                    }
                    .into()
                }
            })
        }
        Ok(Self(Arc::new(list)))
    }
}
