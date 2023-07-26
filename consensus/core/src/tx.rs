use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smallvec::SmallVec;
use std::{
    collections::HashSet,
    fmt::Display,
    ops::Range,
    str::{self, FromStr},
};

use crate::{
    hashing,
    subnets::{self, SubnetworkId},
};

/// COINBASE_TRANSACTION_INDEX is the index of the coinbase transaction in every block
pub const COINBASE_TRANSACTION_INDEX: usize = 0;

/// Size of the underlying script vector of a script.
pub const SCRIPT_VECTOR_SIZE: usize = 36;

/// Represents the ID of a Kaspa transaction
pub type TransactionId = kaspa_hashes::Hash;

/// Used as the underlying type for script public key data, optimized for the common p2pk script size (34).
pub type ScriptVec = SmallVec<[u8; SCRIPT_VECTOR_SIZE]>;

/// Represents the ScriptPublicKey Version
pub type ScriptPublicKeyVersion = u16;

/// Alias the `smallvec!` macro to ease maintenance
pub use smallvec::smallvec as scriptvec;

//Represents a Set of [`ScriptPublicKey`]s
pub type ScriptPublicKeys = HashSet<ScriptPublicKey>;

/// Represents a Kaspad ScriptPublicKey
#[derive(Default, Debug, PartialEq, Eq, Clone, Hash)]
pub struct ScriptPublicKey {
    version: ScriptPublicKeyVersion,
    script: ScriptVec, // Kept private to preserve read-only semantics
}

#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Hash)]
#[serde(rename_all = "camelCase")]
#[serde(rename = "ScriptPublicKey")]
struct ScriptPublicKeyInternal<'a> {
    version: ScriptPublicKeyVersion,
    script: &'a [u8],
}

impl Serialize for ScriptPublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let mut hex = vec![0u8; self.script.len() * 2 + 4];
            faster_hex::hex_encode(&self.version.to_be_bytes(), &mut hex).map_err(serde::ser::Error::custom)?;
            faster_hex::hex_encode(&self.script, &mut hex[4..]).map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(unsafe { str::from_utf8_unchecked(&hex) })
        } else {
            ScriptPublicKeyInternal { version: self.version, script: &self.script }.serialize(serializer)
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for ScriptPublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = <&str as Deserialize>::deserialize(deserializer)?;
            FromStr::from_str(s).map_err(serde::de::Error::custom)
        } else {
            ScriptPublicKeyInternal::deserialize(deserializer)
                .map(|ScriptPublicKeyInternal { script, version }| Self { version, script: SmallVec::from_slice(script) })
        }
    }
}

impl FromStr for ScriptPublicKey {
    type Err = faster_hex::Error;
    fn from_str(hex_str: &str) -> Result<Self, Self::Err> {
        let hex_len = hex_str.len();
        if hex_len < 4 {
            return Err(faster_hex::Error::InvalidLength(hex_len));
        }
        let mut bytes = vec![0u8; hex_len / 2];
        faster_hex::hex_decode(hex_str.as_bytes(), bytes.as_mut_slice())?;
        let version = u16::from_be_bytes(bytes[0..2].try_into().unwrap());
        Ok(Self { version, script: SmallVec::from_slice(&bytes[2..]) })
    }
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
pub struct UtxoEntry {
    pub amount: u64,
    pub script_public_key: ScriptPublicKey,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl UtxoEntry {
    pub fn new(amount: u64, script_public_key: ScriptPublicKey, block_daa_score: u64, is_coinbase: bool) -> Self {
        Self { amount, script_public_key, block_daa_score, is_coinbase }
    }
}

pub type TransactionIndexType = u32;

/// Represents a Kaspa transaction outpoint
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOutpoint {
    pub transaction_id: TransactionId,
    pub index: TransactionIndexType,
}

impl TransactionOutpoint {
    pub fn new(transaction_id: TransactionId, index: u32) -> Self {
        Self { transaction_id, index }
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
pub struct TransactionInput {
    pub previous_outpoint: TransactionOutpoint,
    #[serde(with = "serde_bytes")]
    pub signature_script: Vec<u8>, // TODO: Consider using SmallVec
    pub sequence: u64,
    pub sig_op_count: u8,
}

impl TransactionInput {
    pub fn new(previous_outpoint: TransactionOutpoint, signature_script: Vec<u8>, sequence: u64, sig_op_count: u8) -> Self {
        Self { previous_outpoint, signature_script, sequence, sig_op_count }
    }
}

/// Represents a Kaspad transaction output
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOutput {
    pub value: u64,
    pub script_public_key: ScriptPublicKey,
}

impl TransactionOutput {
    pub fn new(value: u64, script_public_key: ScriptPublicKey) -> Self {
        Self { value, script_public_key }
    }
}

/// Represents a Kaspa transaction
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub version: u16,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub lock_time: u64,
    pub subnetwork_id: SubnetworkId,
    pub gas: u64,
    #[serde(with = "serde_bytes")]
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
    fn test_spk_serde_json() {
        let vec = (0..SCRIPT_VECTOR_SIZE as u8).collect::<Vec<_>>();
        let spk = ScriptPublicKey::from_vec(0xc0de, vec.clone());
        let hex = serde_json::to_string(&spk).unwrap();
        assert_eq!("\"c0de000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20212223\"", hex);
        let spk = serde_json::from_str::<ScriptPublicKey>(&hex).unwrap();
        assert_eq!(spk.version, 0xc0de);
        assert_eq!(spk.script.as_slice(), vec.as_slice());
        let result = "00".parse::<ScriptPublicKey>();
        assert!(matches!(result, Err(faster_hex::Error::InvalidLength(2))));
        let result = "0000".parse::<ScriptPublicKey>();
        let _empty = ScriptPublicKey { version: 0, script: ScriptVec::new() };
        assert!(matches!(result, Ok(_empty)));
    }

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
