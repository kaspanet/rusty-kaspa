use crate::imports::*;
use crate::result::Result;
use crate::runtime::{Account, Balance};
use crate::wasm::tx::{TransactionOutpoint, TransactionOutpointInner};
use itertools::Itertools;
use kaspa_rpc_core::RpcUtxosByAddressesEntry;
use workflow_wasm::abi::ref_from_abi;

use super::UtxoContext;

// thresholds for 1 BPS network
use crate::utxo::{UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA, UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA};

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
            self.block_daa_score() + UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA.load(Ordering::SeqCst) < current_daa_score
        } else {
            self.block_daa_score() + UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA.load(Ordering::SeqCst) < current_daa_score
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub fn entry(&self) -> UtxoEntry {
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

    #[wasm_bindgen(getter)]
    pub fn amount(&self) -> u64 {
        self.utxo.amount()
    }

    #[wasm_bindgen(getter, js_name = "isCoinbase")]
    pub fn is_coinbase(&self) -> bool {
        self.utxo.entry.is_coinbase
    }

    #[wasm_bindgen(getter, js_name = "blockDaaScore")]
    pub fn block_daa_score(&self) -> u64 {
        self.utxo.entry.block_daa_score
    }
}

impl UtxoEntryReference {
    #[inline(always)]
    pub fn id(&self) -> UtxoEntryId {
        self.utxo.outpoint.inner().clone()
    }

    #[inline(always)]
    pub fn id_as_ref(&self) -> &UtxoEntryId {
        self.utxo.outpoint.inner()
    }

    #[inline(always)]
    pub fn amount_as_ref(&self) -> &u64 {
        &self.utxo.entry.amount
    }

    #[inline(always)]
    pub fn transaction_id(&self) -> TransactionId {
        self.utxo.outpoint.transaction_id()
    }

    #[inline(always)]
    pub fn transaction_id_as_ref(&self) -> &TransactionId {
        self.utxo.outpoint.transaction_id_as_ref()
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
        self.id() == other.id()
    }
}

impl Ord for UtxoEntryReference {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id().cmp(&other.id())
    }
}

impl PartialOrd for UtxoEntryReference {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.id().cmp(&other.id()))
    }
}

pub trait TryIntoUtxoEntryReferences {
    fn try_into_utxo_entry_references(&self) -> Result<Vec<UtxoEntryReference>>;
}

impl TryIntoUtxoEntryReferences for JsValue {
    fn try_into_utxo_entry_references(&self) -> Result<Vec<UtxoEntryReference>> {
        Array::from(self).iter().map(UtxoEntryReference::try_from).collect()
    }
}

pub struct PendingUtxoEntryReferenceInner {
    pub entry: UtxoEntryReference,
    pub utxo_context: UtxoContext,
}

#[derive(Clone)]
pub struct PendingUtxoEntryReference {
    pub inner: Arc<PendingUtxoEntryReferenceInner>,
}

impl PendingUtxoEntryReference {
    pub fn new(entry: UtxoEntryReference, utxo_context: UtxoContext) -> Self {
        Self { inner: Arc::new(PendingUtxoEntryReferenceInner { entry, utxo_context }) }
    }

    #[inline(always)]
    pub fn inner(&self) -> &PendingUtxoEntryReferenceInner {
        &self.inner
    }

    #[inline(always)]
    pub fn entry(&self) -> &UtxoEntryReference {
        &self.inner().entry
    }

    #[inline(always)]
    pub fn utxo_context(&self) -> &UtxoContext {
        &self.inner().utxo_context
    }

    #[inline(always)]
    pub fn id(&self) -> UtxoEntryId {
        self.inner().entry.id()
    }

    #[inline(always)]
    pub fn transaction_id(&self) -> TransactionId {
        self.inner().entry.transaction_id()
    }

    #[inline(always)]
    pub fn is_mature(&self, current_daa_score: u64) -> bool {
        self.inner().entry.is_mature(current_daa_score)
    }
}

impl From<(&Arc<dyn Account>, UtxoEntryReference)> for PendingUtxoEntryReference {
    fn from((account, entry): (&Arc<dyn Account>, UtxoEntryReference)) -> Self {
        Self::new(entry, (*account.utxo_context()).clone())
    }
}

impl From<PendingUtxoEntryReference> for UtxoEntryReference {
    fn from(pending: PendingUtxoEntryReference) -> Self {
        pending.inner().entry.clone()
    }
}

// ---

/// A simple collection of UTXO entries. This struct is used to
/// retain a set of UTXO entries in the WASM memory for faster
/// processing. This struct keeps a list of entries represented
/// by `UtxoEntryReference` struct. This data structure is used
/// internally by the framework, but is exposed for convenience.
/// Please consider using `UtxoContect` instead.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen(inspectable)]
pub struct UtxoEntries(Arc<Vec<UtxoEntryReference>>);

impl UtxoEntries {
    pub fn contains(&self, entry: &UtxoEntryReference) -> bool {
        self.0.contains(entry)
    }

    pub fn iter(&self) -> impl Iterator<Item = &UtxoEntryReference> {
        self.0.iter()
    }
}

#[wasm_bindgen]
impl UtxoEntries {
    /// Create a new `UtxoEntries` struct with a set of entries.
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

    /// Sort the contained entries by amount. Please note that
    /// this function is not intended for use with large UTXO sets
    /// as it duplicates the whole contained UTXO set while sorting.
    pub fn sort(&mut self) {
        let mut items = (*self.0).clone();
        items.sort_by_key(|e| e.amount());
        self.0 = Arc::new(items);
    }

    pub fn amount(&self) -> u64 {
        self.0.iter().map(|e| e.amount()).sum()
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

impl From<Vec<UtxoEntryReference>> for UtxoEntries {
    fn from(list: Vec<UtxoEntryReference>) -> Self {
        Self(Arc::new(list))
    }
}

impl TryFrom<JsValue> for UtxoEntries {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        if !js_value.is_array() {
            return Err("Data type spplied to UtxoEntries must be an Array".into());
        }

        Ok(Self(Arc::new(js_value.try_into_utxo_entry_references()?)))
    }
}

impl TryFrom<JsValue> for UtxoEntryReference {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        Self::try_from(&js_value)
    }
}

impl TryFrom<&JsValue> for UtxoEntryReference {
    type Error = Error;
    fn try_from(js_value: &JsValue) -> std::result::Result<Self, Self::Error> {
        if let Ok(utxo_entry) = ref_from_abi!(UtxoEntry, js_value) {
            Ok(Self::from(utxo_entry))
        } else if let Ok(utxo_entry_reference) = ref_from_abi!(UtxoEntryReference, js_value) {
            Ok(utxo_entry_reference)
        } else if let Some(object) = Object::try_from(js_value) {
            let address = Address::try_from(object.get("address")?)?;
            let outpoint = TransactionOutpoint::try_from(object.get("outpoint")?)?;
            let utxo_entry = Object::from(object.get("utxoEntry")?);
            let amount = utxo_entry.get_u64("amount")?;
            let script_public_key = ScriptPublicKey::try_from(utxo_entry.get("scriptPublicKey")?)?;
            let block_daa_score = utxo_entry.get_u64("blockDaaScore")?;
            let is_coinbase = utxo_entry.get_bool("isCoinbase")?;

            let utxo_entry = UtxoEntry {
                address: Some(address),
                outpoint,
                entry: cctx::UtxoEntry { amount, script_public_key, block_daa_score, is_coinbase },
            };

            Ok(UtxoEntryReference::from(utxo_entry))
        } else {
            Err("Data type supplied to UtxoEntryReference must be an object".into())
        }
    }
}
