use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use js_sys::Array;
use kaspa_core::hex::*;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::iter::FromIterator;
use std::{collections::HashSet, fmt::Display, ops::Range};
use wasm_bindgen::prelude::*;
use workflow_wasm::{abi::ref_from_abi, jsvalue::JsValueTrait};

use crate::{
    hashing,
    subnets::{self, SubnetworkId},
};

/// COINBASE_TRANSACTION_INDEX is the index of the coinbase transaction in every block
pub const COINBASE_TRANSACTION_INDEX: usize = 0;

/// Size of the underlying script vector of a script.
pub const SCRIPT_VECTOR_SIZE: usize = 36;

/// Represents the ID of a Kaspa transaction
pub type TransactionId = hashes::Hash;

/// Used as the underlying type for script public key data, optimized for the common p2pk script size (34).
pub type ScriptVec = SmallVec<[u8; SCRIPT_VECTOR_SIZE]>;

/// Represents the ScriptPublicKey Version
pub type ScriptPublicKeyVersion = u16;

/// Alias the `smallvec!` macro to ease maintenance
pub use smallvec::smallvec as scriptvec;

//Represents a Set of [`ScriptPublicKey`]s
pub type ScriptPublicKeys = HashSet<ScriptPublicKey>;

/// Represents a Kaspad ScriptPublicKey
#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Hash)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct ScriptPublicKey {
    pub version: ScriptPublicKeyVersion,
    script: ScriptVec, // Kept private to preserve read-only semantics
}

impl ScriptPublicKey {
    pub fn new(version: ScriptPublicKeyVersion, script: ScriptVec) -> Self {
        Self { version, script }
    }

    pub fn from_vec(version: ScriptPublicKeyVersion, script: Vec<u8>) -> Self {
        Self { version, script: ScriptVec::from_vec(script) }
    }

    pub fn version(&self) -> ScriptPublicKeyVersion {
        self.version
    }

    pub fn script(&self) -> &[u8] {
        &self.script
    }
}

#[wasm_bindgen]
impl ScriptPublicKey {
    #[wasm_bindgen(constructor)]
    pub fn constructor(version: u16, script: JsValue) -> Result<ScriptPublicKey, JsError> {
        let script = script.try_as_vec_u8()?;
        Ok(ScriptPublicKey::new(version, script.into()))
    }

    #[wasm_bindgen(getter = script)]
    pub fn script_as_hex(&self) -> String {
        self.script.to_hex()
    }
}

//
// Borsh serializers need to be manually implemented for `ScriptPublicKey` since
// smallvec does not currently support Borsh
//

impl BorshSerialize for ScriptPublicKey {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.version, writer)?;
        // Vectors and slices are all serialized internally the same way
        borsh::BorshSerialize::serialize(&self.script.as_slice(), writer)?;
        Ok(())
    }
}

impl BorshDeserialize for ScriptPublicKey {
    fn deserialize(buf: &mut &[u8]) -> std::io::Result<Self> {
        // Deserialize into vec first since we have no custom smallvec support
        Ok(Self::from_vec(borsh::BorshDeserialize::deserialize(buf)?, borsh::BorshDeserialize::deserialize(buf)?))
    }
}

impl BorshSchema for ScriptPublicKey {
    fn add_definitions_recursively(
        definitions: &mut std::collections::HashMap<borsh::schema::Declaration, borsh::schema::Definition>,
    ) {
        let fields = borsh::schema::Fields::NamedFields(std::vec![
            ("version".to_string(), <u16>::declaration()),
            ("script".to_string(), <Vec<u8>>::declaration())
        ]);
        let definition = borsh::schema::Definition::Struct { fields };
        Self::add_definition(Self::declaration(), definition, definitions);
        <u16>::add_definitions_recursively(definitions);
        // `<Vec<u8>>` can be safely used as scheme definition for smallvec. See comments above.
        <Vec<u8>>::add_definitions_recursively(definitions);
    }

    fn declaration() -> borsh::schema::Declaration {
        "ScriptPublicKey".to_string()
    }
}

/// Holds details about an individual transaction output in a utxo
/// set such as whether or not it was contained in a coinbase tx, the daa
/// score of the block that accepts the tx, its public key script, and how
/// much it pays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct UtxoEntry {
    pub amount: u64,
    #[wasm_bindgen(js_name = scriptPublicKey, getter_with_clone)]
    pub script_public_key: ScriptPublicKey,
    #[wasm_bindgen(js_name = blockDaaScore)]
    pub block_daa_score: u64,
    #[wasm_bindgen(js_name = isCoinbase)]
    pub is_coinbase: bool,
}

impl UtxoEntry {
    pub fn new(amount: u64, script_public_key: ScriptPublicKey, block_daa_score: u64, is_coinbase: bool) -> Self {
        Self { amount, script_public_key, block_daa_score, is_coinbase }
    }
}

#[wasm_bindgen]
impl UtxoEntry {
    #[wasm_bindgen(constructor)]
    pub fn constructor(amount: u64, script_public_key: &ScriptPublicKey, block_daa_score: u64, is_coinbase: bool) -> Self {
        Self { amount, script_public_key: script_public_key.clone(), block_daa_score, is_coinbase }
    }
}

pub type TransactionIndexType = u32;

/// Represents a Kaspa transaction outpoint
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct TransactionOutpoint {
    #[wasm_bindgen(js_name = transactionId)]
    pub transaction_id: TransactionId,
    pub index: TransactionIndexType,
}

impl TransactionOutpoint {
    pub fn new(transaction_id: TransactionId, index: u32) -> Self {
        Self { transaction_id, index }
    }
}

#[wasm_bindgen]
impl TransactionOutpoint {
    #[wasm_bindgen(constructor)]
    pub fn constructor(transaction_id: &TransactionId, index: u32) -> Self {
        Self { transaction_id: *transaction_id, index }
    }
}

impl Display for TransactionOutpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.transaction_id, self.index)
    }
}

/// Represents a Kaspa transaction input
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct TransactionInput {
    #[wasm_bindgen(js_name = previousOutpoint)]
    pub previous_outpoint: TransactionOutpoint,
    #[wasm_bindgen(skip)]
    pub signature_script: Vec<u8>, // TODO: Consider using SmallVec
    pub sequence: u64,
    #[wasm_bindgen(js_name = sigOpCount)]
    pub sig_op_count: u8,
}

impl TransactionInput {
    pub fn new(previous_outpoint: TransactionOutpoint, signature_script: Vec<u8>, sequence: u64, sig_op_count: u8) -> Self {
        Self { previous_outpoint, signature_script, sequence, sig_op_count }
    }
}

#[wasm_bindgen]
impl TransactionInput {
    #[wasm_bindgen(constructor)]
    pub fn js_ctor(js_value: JsValue) -> Result<TransactionInput, JsError> {
        Ok(js_value.try_into()?)
    }

    #[wasm_bindgen(getter = signatureScript)]
    pub fn get_signature_script_as_hex(&self) -> String {
        self.signature_script.to_hex()
    }

    #[wasm_bindgen(setter = signatureScript)]
    pub fn set_signature_script_from_js_value(&mut self, js_value: JsValue) {
        self.signature_script = js_value.try_as_vec_u8().expect("invalid signature script");
    }
}

/// Represents a Kaspad transaction output
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct TransactionOutput {
    pub value: u64,
    #[wasm_bindgen(js_name = scriptPublicKey, getter_with_clone)]
    pub script_public_key: ScriptPublicKey,
}

impl TransactionOutput {
    pub fn new(value: u64, script_public_key: ScriptPublicKey) -> Self {
        Self { value, script_public_key }
    }
}

#[wasm_bindgen]
impl TransactionOutput {
    #[wasm_bindgen(constructor)]
    /// TransactionOutput constructor
    pub fn constructor(value: u64, script_public_key: &ScriptPublicKey) -> TransactionOutput {
        Self { value, script_public_key: script_public_key.clone() }
    }
}

/// Represents a Kaspa transaction
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct Transaction {
    pub version: u16,
    #[wasm_bindgen(skip)]
    pub inputs: Vec<TransactionInput>,
    #[wasm_bindgen(skip)]
    pub outputs: Vec<TransactionOutput>,
    #[wasm_bindgen(js_name = lockTime)]
    pub lock_time: u64,
    #[wasm_bindgen(skip)]
    pub subnetwork_id: SubnetworkId,
    pub gas: u64,
    #[wasm_bindgen(skip)]
    pub payload: Vec<u8>,

    // A field that is used to cache the transaction ID.
    // Always use the corresponding self.id() instead of accessing this field directly
    id: TransactionId,
}

impl Transaction {
    pub fn new(
        version: u16,
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        lock_time: u64,
        subnetwork_id: SubnetworkId,
        gas: u64,
        payload: Vec<u8>,
    ) -> Self {
        let mut tx = Self {
            version,
            inputs,
            outputs,
            lock_time,
            subnetwork_id,
            gas,
            payload,
            id: Default::default(), // Temp init before the finalize below
        };
        tx.finalize();
        tx
    }
}

#[wasm_bindgen]
impl Transaction {
    /// Determines whether or not a transaction is a coinbase transaction. A coinbase
    /// transaction is a special transaction created by miners that distributes fees and block subsidy
    /// to the previous blocks' miners, and specifies the script_pub_key that will be used to pay the current
    /// miner in future blocks.
    pub fn is_coinbase(&self) -> bool {
        self.subnetwork_id == subnets::SUBNETWORK_ID_COINBASE
    }

    /// Recompute and finalize the tx id based on updated tx fields
    pub fn finalize(&mut self) {
        self.id = hashing::tx::id(self);
    }

    /// Returns the transaction ID
    pub fn id(&self) -> TransactionId {
        self.id
    }
}

#[wasm_bindgen]
impl Transaction {
    #[wasm_bindgen(constructor)]
    pub fn constructor(js_value: JsValue) -> Result<Transaction, JsError> {
        Ok(js_value.try_into()?)
    }

    #[wasm_bindgen(getter = inputs)]
    pub fn get_inputs_as_js_array(&self) -> JsValue {
        let inputs = self.inputs.clone().into_iter().map(<TransactionInput as Into<JsValue>>::into);
        Array::from_iter(inputs).into()
    }

    #[wasm_bindgen(setter = inputs)]
    pub fn set_inputs_from_js_array(&mut self, js_value: &JsValue) {
        let inputs = Array::from(js_value)
            .iter()
            .map(|js_value| {
                ref_from_abi!(TransactionInput, &js_value).unwrap_or_else(|err| panic!("invalid transaction input: {err}"))
            })
            .collect::<Vec<_>>();
        self.inputs = inputs;
    }

    #[wasm_bindgen(getter = outputs)]
    pub fn get_outputs_as_js_array(&self) -> JsValue {
        let outputs = self.outputs.clone().into_iter().map(<TransactionOutput as Into<JsValue>>::into);
        Array::from_iter(outputs).into()
    }

    #[wasm_bindgen(setter = outputs)]
    pub fn set_outputs_from_js_array(&mut self, js_value: &JsValue) {
        let outputs = Array::from(js_value)
            .iter()
            .map(|js_value| {
                ref_from_abi!(TransactionOutput, &js_value).unwrap_or_else(|err| panic!("invalid transaction output: {err}"))
            })
            .collect::<Vec<_>>();
        self.outputs = outputs;
    }

    #[wasm_bindgen(getter = subnetworkId)]
    pub fn get_subnetwork_id_as_hex(&self) -> String {
        self.subnetwork_id.to_hex()
    }

    #[wasm_bindgen(setter = subnetworkId)]
    pub fn set_subnetwork_id_from_js_value(&mut self, js_value: JsValue) {
        let subnetwork_id = js_value.try_as_vec_u8().unwrap_or_else(|err| panic!("subnetwork id error: {err}"));
        self.subnetwork_id = subnetwork_id.as_slice().try_into().unwrap_or_else(|err| panic!("subnetwork id error: {err}"));
    }

    #[wasm_bindgen(getter = payload)]
    pub fn get_payload_as_hex_string(&self) -> String {
        self.payload.to_hex()
    }

    #[wasm_bindgen(setter = payload)]
    pub fn set_payload_from_js_value(&mut self, js_value: JsValue) {
        self.payload = js_value.try_as_vec_u8().unwrap_or_else(|err| panic!("payload value error: {err}"));
    }
}

/// Represents any kind of transaction which has populated UTXO entry data and can be verified/signed etc
pub trait VerifiableTransaction {
    fn tx(&self) -> &Transaction;

    /// Returns the `i`'th populated input
    fn populated_input(&self, index: usize) -> (&TransactionInput, &UtxoEntry);

    /// Returns an iterator over populated `(input, entry)` pairs
    fn populated_inputs(&self) -> PopulatedInputIterator<'_, Self>
    where
        Self: Sized,
    {
        PopulatedInputIterator::new(self)
    }

    fn inputs(&self) -> &[TransactionInput] {
        &self.tx().inputs
    }

    fn outputs(&self) -> &[TransactionOutput] {
        &self.tx().outputs
    }

    fn is_coinbase(&self) -> bool {
        self.tx().is_coinbase()
    }

    fn id(&self) -> TransactionId {
        self.tx().id()
    }
}

/// A custom iterator written only so that `populated_inputs` has a known return type and can de defined on the trait level
pub struct PopulatedInputIterator<'a, T: VerifiableTransaction> {
    tx: &'a T,
    r: Range<usize>,
}

impl<'a, T: VerifiableTransaction> PopulatedInputIterator<'a, T> {
    pub fn new(tx: &'a T) -> Self {
        Self { tx, r: (0..tx.inputs().len()) }
    }
}

impl<'a, T: VerifiableTransaction> Iterator for PopulatedInputIterator<'a, T> {
    type Item = (&'a TransactionInput, &'a UtxoEntry);

    fn next(&mut self) -> Option<Self::Item> {
        self.r.next().map(|i| self.tx.populated_input(i))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.r.size_hint()
    }
}

impl<'a, T: VerifiableTransaction> ExactSizeIterator for PopulatedInputIterator<'a, T> {}

/// Represents a read-only referenced transaction along with fully populated UTXO entry data
pub struct PopulatedTransaction<'a> {
    pub tx: &'a Transaction,
    pub entries: Vec<UtxoEntry>,
}

impl<'a> PopulatedTransaction<'a> {
    pub fn new(tx: &'a Transaction, entries: Vec<UtxoEntry>) -> Self {
        assert_eq!(tx.inputs.len(), entries.len());
        Self { tx, entries }
    }
}

impl<'a> VerifiableTransaction for PopulatedTransaction<'a> {
    fn tx(&self) -> &Transaction {
        self.tx
    }

    fn populated_input(&self, index: usize) -> (&TransactionInput, &UtxoEntry) {
        (&self.tx.inputs[index], &self.entries[index])
    }
}

/// Represents a validated transaction with populated UTXO entry data and a calculated fee
pub struct ValidatedTransaction<'a> {
    pub tx: &'a Transaction,
    pub entries: Vec<UtxoEntry>,
    pub calculated_fee: u64,
}

impl<'a> ValidatedTransaction<'a> {
    pub fn new(populated_tx: PopulatedTransaction<'a>, calculated_fee: u64) -> Self {
        Self { tx: populated_tx.tx, entries: populated_tx.entries, calculated_fee }
    }

    pub fn new_coinbase(tx: &'a Transaction) -> Self {
        assert!(tx.is_coinbase());
        Self { tx, entries: Vec::new(), calculated_fee: 0 }
    }
}

impl<'a> VerifiableTransaction for ValidatedTransaction<'a> {
    fn tx(&self) -> &Transaction {
        self.tx
    }

    fn populated_input(&self, index: usize) -> (&TransactionInput, &UtxoEntry) {
        (&self.tx.inputs[index], &self.entries[index])
    }
}

impl AsRef<Transaction> for Transaction {
    fn as_ref(&self) -> &Transaction {
        self
    }
}

/// Represents a generic mutable/readonly/pointer transaction type along
/// with partially filled UTXO entry data and optional fee and mass
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutableTransaction<T: AsRef<Transaction> = std::sync::Arc<Transaction>> {
    /// The inner transaction
    pub tx: T,
    /// Partially filled UTXO entry data
    pub entries: Vec<Option<UtxoEntry>>,
    /// Populated fee
    pub calculated_fee: Option<u64>,
    /// Populated mass
    pub calculated_mass: Option<u64>,
}

impl<T: AsRef<Transaction>> MutableTransaction<T> {
    pub fn new(tx: T) -> Self {
        let num_inputs = tx.as_ref().inputs.len();
        Self { tx, entries: vec![None; num_inputs], calculated_fee: None, calculated_mass: None }
    }

    pub fn id(&self) -> TransactionId {
        self.tx.as_ref().id()
    }

    pub fn with_entries(tx: T, entries: Vec<UtxoEntry>) -> Self {
        assert_eq!(tx.as_ref().inputs.len(), entries.len());
        Self { tx, entries: entries.into_iter().map(Some).collect(), calculated_fee: None, calculated_mass: None }
    }

    /// Returns the tx wrapped as a [`VerifiableTransaction`]. Note that this function
    /// must be called only once all UTXO entries are populated, otherwise it panics.
    pub fn as_verifiable(&self) -> impl VerifiableTransaction + '_ {
        assert!(self.is_verifiable());
        MutableTransactionVerifiableWrapper { inner: self }
    }

    pub fn is_verifiable(&self) -> bool {
        assert_eq!(self.entries.len(), self.tx.as_ref().inputs.len());
        self.entries.iter().all(|e| e.is_some())
    }

    pub fn is_fully_populated(&self) -> bool {
        self.is_verifiable() && self.calculated_fee.is_some() && self.calculated_mass.is_some()
    }

    pub fn missing_outpoints(&self) -> impl Iterator<Item = TransactionOutpoint> + '_ {
        assert_eq!(self.entries.len(), self.tx.as_ref().inputs.len());
        self.entries.iter().enumerate().filter_map(|(i, entry)| {
            if entry.is_none() {
                Some(self.tx.as_ref().inputs[i].previous_outpoint)
            } else {
                None
            }
        })
    }

    pub fn clear_entries(&mut self) {
        for entry in self.entries.iter_mut() {
            *entry = None;
        }
    }
}

/// Private struct used to wrap a [`MutableTransaction`] as a [`VerifiableTransaction`]
struct MutableTransactionVerifiableWrapper<'a, T: AsRef<Transaction>> {
    inner: &'a MutableTransaction<T>,
}

impl<T: AsRef<Transaction>> VerifiableTransaction for MutableTransactionVerifiableWrapper<'_, T> {
    fn tx(&self) -> &Transaction {
        self.inner.tx.as_ref()
    }

    fn populated_input(&self, index: usize) -> (&TransactionInput, &UtxoEntry) {
        (
            &self.inner.tx.as_ref().inputs[index],
            self.inner.entries[index].as_ref().expect("expected to be called only following full UTXO population"),
        )
    }
}

/// Specialized impl for `T=Arc<Transaction>`
impl MutableTransaction {
    pub fn from_tx(tx: Transaction) -> Self {
        Self::new(std::sync::Arc::new(tx))
    }
}

/// Alias for a fully mutable and owned transaction which can be populated with external data
/// and can also be modified internally and signed etc.
pub type SignableTransaction = MutableTransaction<Transaction>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spk_borsh() {
        // Tests for ScriptPublicKey Borsh ser/deser since we manually implemented them
        let spk = ScriptPublicKey::from_vec(12, vec![32; 20]);
        let bin = spk.try_to_vec().unwrap();
        let spk2: ScriptPublicKey = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(spk, spk2);

        let spk = ScriptPublicKey::from_vec(55455, vec![11; 200]);
        let bin = spk.try_to_vec().unwrap();
        let spk2: ScriptPublicKey = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(spk, spk2);
    }
}
