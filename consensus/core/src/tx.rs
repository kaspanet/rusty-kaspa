mod script_public_key;

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_utils::hex::ToHex;
use kaspa_utils::mem_size::MemSizeEstimator;
use kaspa_utils::{serde_bytes, serde_bytes_fixed_ref};
pub use script_public_key::{
    scriptvec, ScriptPublicKey, ScriptPublicKeyT, ScriptPublicKeyVersion, ScriptPublicKeys, ScriptVec, SCRIPT_VECTOR_SIZE,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::SeqCst;
use std::{
    fmt::Display,
    ops::Range,
    str::{self},
};
use wasm_bindgen::prelude::*;

use crate::{
    hashing,
    subnets::{self, SubnetworkId},
};

/// COINBASE_TRANSACTION_INDEX is the index of the coinbase transaction in every block
pub const COINBASE_TRANSACTION_INDEX: usize = 0;
pub type TransactionId = kaspa_hashes::Hash;

/// Holds details about an individual transaction output in a utxo
/// set such as whether or not it was contained in a coinbase tx, the daa
/// score of the block that accepts the tx, its public key script, and how
/// much it pays.
/// @category Consensus
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable, js_name = TransactionUtxoEntry)]
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

impl MemSizeEstimator for UtxoEntry {}

pub type TransactionIndexType = u32;

/// Represents a Kaspa transaction outpoint
#[derive(Eq, Default, Hash, PartialEq, Debug, Copy, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOutpoint {
    #[serde(with = "serde_bytes_fixed_ref")]
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
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInput {
    pub previous_outpoint: TransactionOutpoint,
    #[serde(with = "serde_bytes")]
    pub signature_script: Vec<u8>, // TODO: Consider using SmallVec
    pub sequence: u64,

    // TODO: Since this field is used for calculating mass context free, and we already commit
    // to the mass in a dedicated field (on the tx level), it follows that this field is no longer
    // needed, and can be removed if we ever implement a v2 transaction
    pub sig_op_count: u8,
}

impl TransactionInput {
    pub fn new(previous_outpoint: TransactionOutpoint, signature_script: Vec<u8>, sequence: u64, sig_op_count: u8) -> Self {
        Self { previous_outpoint, signature_script, sequence, sig_op_count }
    }
}

impl std::fmt::Debug for TransactionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionInput")
            .field("previous_outpoint", &self.previous_outpoint)
            .field("signature_script", &self.signature_script.to_hex())
            .field("sequence", &self.sequence)
            .field("sig_op_count", &self.sig_op_count)
            .finish()
    }
}

/// Represents a Kaspad transaction output
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TransactionMass(AtomicU64); // TODO: using atomic as a temp solution for mutating this field through the mempool

impl Eq for TransactionMass {}

impl PartialEq for TransactionMass {
    fn eq(&self, other: &Self) -> bool {
        self.0.load(SeqCst) == other.0.load(SeqCst)
    }
}

impl Clone for TransactionMass {
    fn clone(&self) -> Self {
        Self(AtomicU64::new(self.0.load(SeqCst)))
    }
}

impl BorshDeserialize for TransactionMass {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mass: u64 = borsh::BorshDeserialize::deserialize_reader(reader)?;
        Ok(Self(AtomicU64::new(mass)))
    }
}

impl BorshSerialize for TransactionMass {
    fn serialize<W: std::io::prelude::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.0.load(SeqCst), writer)
    }
}

/// Represents a Kaspa transaction
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
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

    #[serde(default)]
    mass: TransactionMass,

    // A field that is used to cache the transaction ID.
    // Always use the corresponding self.id() instead of accessing this field directly
    #[serde(with = "serde_bytes_fixed_ref")]
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
        let mut tx = Self::new_non_finalized(version, inputs, outputs, lock_time, subnetwork_id, gas, payload);
        tx.finalize();
        tx
    }

    pub fn new_non_finalized(
        version: u16,
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        lock_time: u64,
        subnetwork_id: SubnetworkId,
        gas: u64,
        payload: Vec<u8>,
    ) -> Self {
        Self { version, inputs, outputs, lock_time, subnetwork_id, gas, payload, mass: Default::default(), id: Default::default() }
    }
}

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

    /// Set the mass field of this transaction. The mass field is expected depending on hard-forks which are currently
    /// activated only on some testnets. The field has no effect on tx ID so no need to finalize following this call.
    pub fn set_mass(&self, mass: u64) {
        self.mass.0.store(mass, SeqCst)
    }

    pub fn mass(&self) -> u64 {
        self.mass.0.load(SeqCst)
    }

    pub fn with_mass(self, mass: u64) -> Self {
        self.set_mass(mass);
        self
    }
}

impl MemSizeEstimator for Transaction {
    fn estimate_mem_bytes(&self) -> usize {
        // Calculates mem bytes of the transaction (for cache tracking purposes)
        size_of::<Self>()
            + self.payload.len()
            + self
                .inputs
                .iter()
                .map(|i| i.signature_script.len() + size_of::<TransactionInput>())
                .chain(self.outputs.iter().map(|o| {
                    // size_of::<TransactionOutput>() already counts SCRIPT_VECTOR_SIZE bytes within, so we only add the delta
                    o.script_public_key.script().len().saturating_sub(SCRIPT_VECTOR_SIZE) + size_of::<TransactionOutput>()
                }))
                .sum::<usize>()
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
    /// Populated compute mass (does not include the storage mass)
    pub calculated_compute_mass: Option<u64>,
}

impl<T: AsRef<Transaction>> MutableTransaction<T> {
    pub fn new(tx: T) -> Self {
        let num_inputs = tx.as_ref().inputs.len();
        Self { tx, entries: vec![None; num_inputs], calculated_fee: None, calculated_compute_mass: None }
    }

    pub fn id(&self) -> TransactionId {
        self.tx.as_ref().id()
    }

    pub fn with_entries(tx: T, entries: Vec<UtxoEntry>) -> Self {
        assert_eq!(tx.as_ref().inputs.len(), entries.len());
        Self { tx, entries: entries.into_iter().map(Some).collect(), calculated_fee: None, calculated_compute_mass: None }
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
        self.is_verifiable() && self.calculated_fee.is_some() && self.calculated_compute_mass.is_some()
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

    /// Returns the calculated feerate. The feerate is calculated as the amount of fee
    /// this transactions pays per gram of the full contextual (compute & storage) mass. The
    /// function returns a value when calculated fee exists and the contextual mass is greater
    /// than zero, otherwise `None` is returned.
    pub fn calculated_feerate(&self) -> Option<f64> {
        let contextual_mass = self.tx.as_ref().mass();
        if contextual_mass > 0 {
            self.calculated_fee.map(|fee| fee as f64 / contextual_mass as f64)
        } else {
            None
        }
    }

    /// A function for estimating the amount of memory bytes used by this transaction (dedicated to mempool usage).
    /// We need consistency between estimation calls so only this function should be used for this purpose since
    /// `estimate_mem_bytes` is sensitive to pointer wrappers such as Arc
    pub fn mempool_estimated_bytes(&self) -> usize {
        self.estimate_mem_bytes()
    }

    pub fn has_parent(&self, possible_parent: TransactionId) -> bool {
        self.tx.as_ref().inputs.iter().any(|x| x.previous_outpoint.transaction_id == possible_parent)
    }

    pub fn has_parent_in_set(&self, possible_parents: &HashSet<TransactionId>) -> bool {
        self.tx.as_ref().inputs.iter().any(|x| possible_parents.contains(&x.previous_outpoint.transaction_id))
    }
}

impl<T: AsRef<Transaction>> MemSizeEstimator for MutableTransaction<T> {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
            + self
                .entries
                .iter()
                .map(|op| {
                    // size_of::<Option<UtxoEntry>>() already counts SCRIPT_VECTOR_SIZE bytes within, so we only add the delta
                    size_of::<Option<UtxoEntry>>()
                        + op.as_ref().map_or(0, |e| e.script_public_key.script().len().saturating_sub(SCRIPT_VECTOR_SIZE))
                })
                .sum::<usize>()
            + self.tx.as_ref().estimate_mem_bytes()
    }
}

impl<T: AsRef<Transaction>> AsRef<Transaction> for MutableTransaction<T> {
    fn as_ref(&self) -> &Transaction {
        self.tx.as_ref()
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
    use consensus_core::subnets::SUBNETWORK_ID_COINBASE;
    use smallvec::smallvec;

    fn test_transaction() -> Transaction {
        let script_public_key = ScriptPublicKey::new(
            0,
            smallvec![
                0x76, 0xa9, 0x21, 0x03, 0x2f, 0x7e, 0x43, 0x0a, 0xa4, 0xc9, 0xd1, 0x59, 0x43, 0x7e, 0x84, 0xb9, 0x75, 0xdc, 0x76,
                0xd9, 0x00, 0x3b, 0xf0, 0x92, 0x2c, 0xf3, 0xaa, 0x45, 0x28, 0x46, 0x4b, 0xab, 0x78, 0x0d, 0xba, 0x5e
            ],
        );
        Transaction::new(
            1,
            vec![
                TransactionInput {
                    previous_outpoint: TransactionOutpoint {
                        transaction_id: TransactionId::from_slice(&[
                            0x16, 0x5e, 0x38, 0xe8, 0xb3, 0x91, 0x45, 0x95, 0xd9, 0xc6, 0x41, 0xf3, 0xb8, 0xee, 0xc2, 0xf3, 0x46,
                            0x11, 0x89, 0x6b, 0x82, 0x1a, 0x68, 0x3b, 0x7a, 0x4e, 0xde, 0xfe, 0x2c, 0x00, 0x00, 0x00,
                        ]),
                        index: 0xfffffffa,
                    },
                    signature_script: vec![
                        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
                        0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
                    ],
                    sequence: 2,
                    sig_op_count: 3,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint {
                        transaction_id: TransactionId::from_slice(&[
                            0x4b, 0xb0, 0x75, 0x35, 0xdf, 0xd5, 0x8e, 0x0b, 0x3c, 0xd6, 0x4f, 0xd7, 0x15, 0x52, 0x80, 0x87, 0x2a,
                            0x04, 0x71, 0xbc, 0xf8, 0x30, 0x95, 0x52, 0x6a, 0xce, 0x0e, 0x38, 0xc6, 0x00, 0x00, 0x00,
                        ]),
                        index: 0xfffffffb,
                    },
                    signature_script: vec![
                        0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31,
                        0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
                    ],
                    sequence: 4,
                    sig_op_count: 5,
                },
            ],
            vec![
                TransactionOutput { value: 6, script_public_key: script_public_key.clone() },
                TransactionOutput { value: 7, script_public_key },
            ],
            8,
            SUBNETWORK_ID_COINBASE,
            9,
            vec![
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12,
                0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25,
                0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38,
                0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b,
                0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e,
                0x5f, 0x60, 0x61, 0x62, 0x63,
            ],
        )
    }

    #[test]
    fn test_transaction_bincode() {
        let tx = test_transaction();
        let bts = bincode::serialize(&tx).unwrap();

        // standard, based on https://github.com/kaspanet/rusty-kaspa/commit/7e947a06d2434daf4bc7064d4cd87dc1984b56fe
        let expected_bts = vec![
            1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 22, 94, 56, 232, 179, 145, 69, 149, 217, 198, 65, 243, 184, 238, 194, 243, 70, 17, 137, 107,
            130, 26, 104, 59, 122, 78, 222, 254, 44, 0, 0, 0, 250, 255, 255, 255, 32, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8,
            9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 2, 0, 0, 0, 0, 0, 0, 0, 3, 75,
            176, 117, 53, 223, 213, 142, 11, 60, 214, 79, 215, 21, 82, 128, 135, 42, 4, 113, 188, 248, 48, 149, 82, 106, 206, 14, 56,
            198, 0, 0, 0, 251, 255, 255, 255, 32, 0, 0, 0, 0, 0, 0, 0, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
            48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 4, 0, 0, 0, 0, 0, 0, 0, 5, 2, 0, 0, 0, 0, 0, 0, 0, 6, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 36, 0, 0, 0, 0, 0, 0, 0, 118, 169, 33, 3, 47, 126, 67, 10, 164, 201, 209, 89, 67, 126, 132, 185,
            117, 220, 118, 217, 0, 59, 240, 146, 44, 243, 170, 69, 40, 70, 75, 171, 120, 13, 186, 94, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            36, 0, 0, 0, 0, 0, 0, 0, 118, 169, 33, 3, 47, 126, 67, 10, 164, 201, 209, 89, 67, 126, 132, 185, 117, 220, 118, 217, 0,
            59, 240, 146, 44, 243, 170, 69, 40, 70, 75, 171, 120, 13, 186, 94, 8, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 0, 100, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
            13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42,
            43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72,
            73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 0, 0, 0, 0, 0,
            0, 0, 0, 69, 146, 193, 64, 98, 49, 45, 0, 77, 32, 25, 122, 77, 15, 211, 252, 61, 210, 82, 177, 39, 153, 127, 33, 188, 172,
            138, 38, 67, 75, 241, 176,
        ];
        assert_eq!(expected_bts, bts);
        assert_eq!(tx, bincode::deserialize(&bts).unwrap());
    }

    #[test]
    fn test_transaction_json() {
        let tx = test_transaction();
        let str = serde_json::to_string_pretty(&tx).unwrap();
        let expected_str = r#"{
  "version": 1,
  "inputs": [
    {
      "previousOutpoint": {
        "transactionId": "165e38e8b3914595d9c641f3b8eec2f34611896b821a683b7a4edefe2c000000",
        "index": 4294967290
      },
      "signatureScript": "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
      "sequence": 2,
      "sigOpCount": 3
    },
    {
      "previousOutpoint": {
        "transactionId": "4bb07535dfd58e0b3cd64fd7155280872a0471bcf83095526ace0e38c6000000",
        "index": 4294967291
      },
      "signatureScript": "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f",
      "sequence": 4,
      "sigOpCount": 5
    }
  ],
  "outputs": [
    {
      "value": 6,
      "scriptPublicKey": "000076a921032f7e430aa4c9d159437e84b975dc76d9003bf0922cf3aa4528464bab780dba5e"
    },
    {
      "value": 7,
      "scriptPublicKey": "000076a921032f7e430aa4c9d159437e84b975dc76d9003bf0922cf3aa4528464bab780dba5e"
    }
  ],
  "lockTime": 8,
  "subnetworkId": "0100000000000000000000000000000000000000",
  "gas": 9,
  "payload": "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f60616263",
  "mass": 0,
  "id": "4592c14062312d004d20197a4d0fd3fc3dd252b127997f21bcac8a26434bf1b0"
}"#;
        assert_eq!(expected_str, str);
        assert_eq!(tx, serde_json::from_str(&str).unwrap());
    }

    #[test]
    fn test_spk_serde_json() {
        let vec = (0..SCRIPT_VECTOR_SIZE as u8).collect::<Vec<_>>();
        let spk = ScriptPublicKey::from_vec(0xc0de, vec.clone());
        let hex: String = serde_json::to_string(&spk).unwrap();
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
        let bin = borsh::to_vec(&spk).unwrap();
        let spk2: ScriptPublicKey = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(spk, spk2);

        let spk = ScriptPublicKey::from_vec(55455, vec![11; 200]);
        let bin = borsh::to_vec(&spk).unwrap();
        let spk2: ScriptPublicKey = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(spk, spk2);
    }

    // use wasm_bindgen_test::wasm_bindgen_test;
    // #[wasm_bindgen_test]
    // pub fn test_wasm_serde_spk_constructor() {
    //     let str = "kaspa:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j";
    //     let a = Address::constructor(str);
    //     let value = to_value(&a).unwrap();
    //
    //     assert_eq!(JsValue::from_str("string"), value.js_typeof());
    //     assert_eq!(value, JsValue::from_str(str));
    //     assert_eq!(a, from_value(value).unwrap());
    // }
    //
    // #[wasm_bindgen_test]
    // pub fn test_wasm_js_serde_spk_object() {
    //     let expected = Address::constructor("kaspa:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j");
    //
    //     use web_sys::console;
    //     console::log_4(&"address: ".into(), &expected.version().into(), &expected.prefix().into(), &expected.payload().into());
    //
    //     let obj = Object::new();
    //     obj.set("version", &JsValue::from_str("PubKey")).unwrap();
    //     obj.set("prefix", &JsValue::from_str("kaspa")).unwrap();
    //     obj.set("payload", &JsValue::from_str("qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j")).unwrap();
    //
    //     assert_eq!(JsValue::from_str("object"), obj.js_typeof());
    //
    //     let obj_js = obj.into_js_result().unwrap();
    //     let actual = from_value(obj_js).unwrap();
    //     assert_eq!(expected, actual);
    // }
    //
    // #[wasm_bindgen_test]
    // pub fn test_wasm_serde_spk_object() {
    //     use wasm_bindgen::convert::IntoWasmAbi;
    //
    //     let expected = Address::constructor("kaspa:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j");
    //     let wasm_js_value: JsValue = expected.clone().into_abi().into();
    //
    //     // use web_sys::console;
    //     // console::log_4(&"address: ".into(), &expected.version().into(), &expected.prefix().into(), &expected.payload().into());
    //
    //     let actual = from_value(wasm_js_value).unwrap();
    //     assert_eq!(expected, actual);
    // }
}
