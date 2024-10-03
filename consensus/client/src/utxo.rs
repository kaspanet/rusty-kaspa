//!
//! # UTXO client-side data structures.
//!
//! This module provides client-side data structures for UTXO management.
//! In particular, the [`UtxoEntry`] and [`UtxoEntryReference`] structs
//! are used to represent UTXO entries in the wallet subsystem and WASM bindings.
//!

#![allow(non_snake_case)]

use crate::imports::*;
use crate::outpoint::{TransactionOutpoint, TransactionOutpointInner};
use crate::result::Result;
use kaspa_addresses::Address;

#[wasm_bindgen(typescript_custom_section)]
const TS_UTXO_ENTRY: &'static str = r#"
/**
 * Interface defines the structure of a UTXO entry.
 * 
 * @category Consensus
 */
export interface IUtxoEntry {
    /** @readonly */
    address?: Address;
    /** @readonly */
    outpoint: ITransactionOutpoint;
    /** @readonly */
    amount : bigint;
    /** @readonly */
    scriptPublicKey : IScriptPublicKey;
    /** @readonly */
    blockDaaScore: bigint;
    /** @readonly */
    isCoinbase: boolean;
}

"#;

#[wasm_bindgen]
extern "C" {
    /// WASM type representing an array of [`UtxoEntryReference`] objects (i.e. `UtxoEntryReference[]`)
    #[wasm_bindgen(extends = Array, typescript_type = "UtxoEntryReference[]")]
    pub type UtxoEntryReferenceArrayT;
    /// WASM type representing a UTXO entry interface (a UTXO-like object)
    #[wasm_bindgen(typescript_type = "IUtxoEntry")]
    pub type IUtxoEntry;
    /// WASM type representing an array of UTXO entries (i.e. `IUtxoEntry[]`)
    #[wasm_bindgen(typescript_type = "IUtxoEntry[]")]
    pub type IUtxoEntryArray;
}

/// A UTXO entry Id is a unique identifier for a UTXO entry defined by the `txid+output_index`.
pub type UtxoEntryId = TransactionOutpointInner;

/// [`UtxoEntry`] struct represents a client-side UTXO entry.
///
/// @category Wallet SDK
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct UtxoEntry {
    #[wasm_bindgen(getter_with_clone)]
    pub address: Option<Address>,
    #[wasm_bindgen(getter_with_clone)]
    pub outpoint: TransactionOutpoint,
    pub amount: u64,
    #[wasm_bindgen(js_name = scriptPublicKey, getter_with_clone)]
    pub script_public_key: ScriptPublicKey,
    #[wasm_bindgen(js_name = blockDaaScore)]
    pub block_daa_score: u64,
    #[wasm_bindgen(js_name = isCoinbase)]
    pub is_coinbase: bool,
}

#[wasm_bindgen]
impl UtxoEntry {
    #[wasm_bindgen(js_name = toString)]
    pub fn js_to_string(&self) -> Result<js_sys::JsString> {
        //SerializableUtxoEntry::from(self).serialize_to_json()
        Ok(js_sys::JSON::stringify(&self.to_js_object()?.into())?)
    }
}

impl UtxoEntry {
    #[inline(always)]
    pub fn amount(&self) -> u64 {
        self.amount
    }
    #[inline(always)]
    pub fn block_daa_score(&self) -> u64 {
        self.block_daa_score
    }

    #[inline(always)]
    pub fn is_coinbase(&self) -> bool {
        self.is_coinbase
    }

    fn to_js_object(&self) -> Result<js_sys::Object> {
        let obj = js_sys::Object::new();
        if let Some(address) = &self.address {
            obj.set("address", &address.to_string().into())?;
        }

        let outpoint = js_sys::Object::new();
        outpoint.set("transactionId", &self.outpoint.transaction_id().to_string().into())?;
        outpoint.set("index", &self.outpoint.index().into())?;

        obj.set("amount", &self.amount.to_string().into())?;
        obj.set("outpoint", &outpoint.into())?;
        obj.set("scriptPublicKey", &workflow_wasm::serde::to_value(&self.script_public_key)?)?;
        obj.set("blockDaaScore", &self.block_daa_score.to_string().into())?;
        obj.set("isCoinbase", &self.is_coinbase.into())?;

        Ok(obj)
    }
}

impl AsRef<UtxoEntry> for UtxoEntry {
    fn as_ref(&self) -> &UtxoEntry {
        self
    }
}

impl From<&UtxoEntry> for cctx::UtxoEntry {
    fn from(utxo: &UtxoEntry) -> Self {
        cctx::UtxoEntry {
            amount: utxo.amount,
            script_public_key: utxo.script_public_key.clone(),
            block_daa_score: utxo.block_daa_score,
            is_coinbase: utxo.is_coinbase,
        }
        // value.entry.clone()
    }
}

/// [`Arc`] reference to a [`UtxoEntry`] used by the wallet subsystems.
///
/// @category Wallet SDK
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct UtxoEntryReference {
    #[wasm_bindgen(skip)]
    pub utxo: Arc<UtxoEntry>,
}

#[wasm_bindgen]
impl UtxoEntryReference {
    #[wasm_bindgen(js_name = toString)]
    pub fn js_to_string(&self) -> Result<js_sys::JsString> {
        //let entry = workflow_wasm::serde::to_value(&SerializableUtxoEntry::from(self))?;
        let object = js_sys::Object::new();
        object.set("entry", &self.utxo.to_js_object()?.into())?;
        Ok(js_sys::JSON::stringify(&object)?)
    }

    #[wasm_bindgen(getter)]
    pub fn entry(&self) -> UtxoEntry {
        self.as_ref().clone()
    }

    #[wasm_bindgen(getter)]
    pub fn outpoint(&self) -> TransactionOutpoint {
        self.utxo.outpoint.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn address(&self) -> Option<Address> {
        self.utxo.address.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn amount(&self) -> u64 {
        self.utxo.amount()
    }

    #[wasm_bindgen(getter, js_name = "isCoinbase")]
    pub fn is_coinbase(&self) -> bool {
        self.utxo.is_coinbase
    }

    #[wasm_bindgen(getter, js_name = "blockDaaScore")]
    pub fn block_daa_score(&self) -> u64 {
        self.utxo.block_daa_score
    }

    #[wasm_bindgen(getter, js_name = "scriptPublicKey")]
    pub fn script_public_key(&self) -> ScriptPublicKey {
        self.utxo.script_public_key.clone()
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
        &self.utxo.amount
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

impl std::hash::Hash for UtxoEntryReference {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
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

impl From<&UtxoEntryReference> for cctx::UtxoEntry {
    fn from(value: &UtxoEntryReference) -> Self {
        value.utxo.as_ref().into()
        // (*value.utxo).clone()
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

/// An extension trait to convert a JS value into a vec of UTXO entry references.
pub trait TryIntoUtxoEntryReferences {
    fn try_into_utxo_entry_references(&self) -> Result<Vec<UtxoEntryReference>>;
}

impl TryIntoUtxoEntryReferences for JsValue {
    fn try_into_utxo_entry_references(&self) -> Result<Vec<UtxoEntryReference>> {
        Array::from(self).iter().map(UtxoEntryReference::try_owned_from).collect()
    }
}

impl TryCastFromJs for UtxoEntry {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Ok(Self::try_ref_from_js_value_as_cast(value)?)
    }
}

/// A simple collection of UTXO entries. This struct is used to
/// retain a set of UTXO entries in the WASM memory for faster
/// processing. This struct keeps a list of entries represented
/// by `UtxoEntryReference` struct. This data structure is used
/// internally by the framework, but is exposed for convenience.
/// Please consider using `UtxoContext` instead.
/// @category Wallet SDK
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
                UtxoEntryReference::try_owned_from(&js_value).unwrap_or_else(|err| panic!("invalid UtxoEntryReference: {err}"))
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
        value.0.as_ref().iter().map(|entry| Some(entry.as_ref().clone())).collect::<Vec<_>>()
    }
}

impl From<Vec<UtxoEntry>> for UtxoEntries {
    fn from(items: Vec<UtxoEntry>) -> Self {
        Self(Arc::new(items.into_iter().map(UtxoEntryReference::from).collect::<_>()))
    }
}

impl From<UtxoEntries> for Vec<Option<cctx::UtxoEntry>> {
    fn from(value: UtxoEntries) -> Self {
        value.0.as_ref().iter().map(|entry| Some(entry.utxo.as_ref().into())).collect::<Vec<_>>()
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
            return Err("Data type supplied to UtxoEntries must be an Array".into());
        }

        Ok(Self(Arc::new(js_value.try_into_utxo_entry_references()?)))
    }
}

impl TryCastFromJs for UtxoEntryReference {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            if let Ok(utxo_entry) = UtxoEntry::try_ref_from_js_value(&value) {
                Ok(Self::from(utxo_entry.clone()))
            } else if let Some(object) = Object::try_from(value.as_ref()) {
                let address = object.try_cast_into::<Address>("address")?;
                let outpoint = TransactionOutpoint::try_from(object.get_value("outpoint")?.as_ref())?;
                let utxo_entry = Object::from(object.get_value("utxoEntry")?);

                let utxo_entry = if !utxo_entry.is_undefined() {
                    let amount = utxo_entry.get_u64("amount").map_err(|_| {
                        Error::custom("Supplied object does not contain `utxoEntry.amount` property (or it is not a numerical value)")
                    })?;
                    let script_public_key = ScriptPublicKey::try_owned_from(utxo_entry.get_value("scriptPublicKey")?)
                        .map_err(|_|Error::custom("Supplied object does not contain `utxoEntry.scriptPublicKey` property (or it is not a hex string or a ScriptPublicKey class)"))?;
                    let block_daa_score = utxo_entry.get_u64("blockDaaScore").map_err(|_| {
                        Error::custom(
                            "Supplied object does not contain `utxoEntry.blockDaaScore` property (or it is not a numerical value)",
                        )
                    })?;
                    let is_coinbase = utxo_entry.get_bool("isCoinbase")?;

                    UtxoEntry { address, outpoint, amount, script_public_key, block_daa_score, is_coinbase }
                } else {
                    let amount = object.get_u64("amount").map_err(|_| {
                        Error::custom("Supplied object does not contain `amount` property (or it is not a numerical value)")
                    })?;
                    let script_public_key = ScriptPublicKey::try_owned_from(object.get_value("scriptPublicKey")?)
                        .map_err(|_|Error::custom("Supplied object does not contain `scriptPublicKey` property (or it is not a hex string or a ScriptPublicKey class)"))?;
                    let block_daa_score = object.get_u64("blockDaaScore").map_err(|_| {
                        Error::custom("Supplied object does not contain `blockDaaScore` property (or it is not a numerical value)")
                    })?;
                    let is_coinbase = object.try_get_bool("isCoinbase")?.unwrap_or(false);

                    UtxoEntry { address, outpoint, amount, script_public_key, block_daa_score, is_coinbase }
                };

                Ok(UtxoEntryReference::from(utxo_entry))
            } else {
                Err("Data type supplied to UtxoEntryReference must be an object".into())
            }
        })
    }
}

impl UtxoEntryReference {
    pub fn simulated(amount: u64) -> Self {
        use kaspa_addresses::{Prefix, Version};
        let address = Address::new(Prefix::Testnet, Version::PubKey, &rand::random::<[u8; 32]>());
        Self::simulated_with_address(amount, &address)
    }

    pub fn simulated_with_address(amount: u64, address: &Address) -> Self {
        let outpoint = TransactionOutpoint::simulated();
        let script_public_key = kaspa_txscript::pay_to_address_script(address);
        let block_daa_score = 0;
        let is_coinbase = false;

        let utxo_entry =
            UtxoEntry { address: Some(address.clone()), outpoint, amount, script_public_key, block_daa_score, is_coinbase };

        UtxoEntryReference::from(utxo_entry)
    }
}
