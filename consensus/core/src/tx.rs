use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::fmt::Display;

use crate::{
    hashing,
    subnets::{self, SubnetworkId},
};

/// Represents the ID of a Kaspa transaction
pub type TransactionId = hashes::Hash;

/// Used as the underlying type for script public key data, optimized for the common p2pk script size (34).
pub type ScriptVec = SmallVec<[u8; 36]>;

/// Alias the `smallvec!` macro to ease maintenance
pub use smallvec::smallvec as scriptvec;

/// Represents a Kaspad ScriptPublicKey
#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScriptPublicKey {
    version: u16,
    script: ScriptVec, // Kept private to preserve read-only semantics
}

impl ScriptPublicKey {
    pub fn new(version: u16, script: ScriptVec) -> Self {
        Self { version, script }
    }

    pub fn from_vec(version: u16, script: Vec<u8>) -> Self {
        Self { version, script: ScriptVec::from_vec(script) }
    }

    pub fn version(&self) -> u16 {
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
#[derive(Debug, Serialize, Deserialize, Clone, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInput {
    pub previous_outpoint: TransactionOutpoint,
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
#[derive(Debug, Serialize, Deserialize, Clone, BorshSerialize, BorshDeserialize, BorshSchema)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub version: u16,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub lock_time: u64,
    pub subnetwork_id: SubnetworkId,
    pub gas: u64,
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

    pub fn finalize(&mut self) {
        self.id = hashing::tx::id(self);
    }

    /// Returns the transaction ID
    pub fn id(&self) -> TransactionId {
        self.id
    }
}

/// Represents a transaction with populated UTXO entry data
pub struct PopulatedTransaction<'a> {
    pub tx: &'a Transaction,
    pub entries: Vec<UtxoEntry>,
}

impl<'a> PopulatedTransaction<'a> {
    pub fn new(tx: &'a Transaction, entries: Vec<UtxoEntry>) -> Self {
        assert_eq!(tx.inputs.len(), entries.len());
        Self { tx, entries }
    }

    pub fn populated_inputs(&self) -> impl ExactSizeIterator<Item = (&TransactionInput, &UtxoEntry)> {
        self.tx.inputs.iter().zip(self.entries.iter())
    }

    pub fn populated_input(&self, index: usize) -> (&TransactionInput, &UtxoEntry) {
        (&self.tx.inputs[index], &self.entries[index])
    }

    pub fn outputs(&self) -> &[TransactionOutput] {
        &self.tx.outputs
    }

    pub fn is_coinbase(&self) -> bool {
        self.tx.is_coinbase()
    }

    pub fn id(&self) -> TransactionId {
        self.tx.id()
    }

    pub fn to_validated(self, calculated_fee: u64) -> ValidatedTransaction<'a> {
        ValidatedTransaction::new(self, calculated_fee)
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

    pub fn populated_inputs(&self) -> impl ExactSizeIterator<Item = (&TransactionInput, &UtxoEntry)> {
        self.tx.inputs.iter().zip(self.entries.iter())
    }

    pub fn outputs(&self) -> &[TransactionOutput] {
        &self.tx.outputs
    }

    pub fn is_coinbase(&self) -> bool {
        self.tx.is_coinbase()
    }

    pub fn id(&self) -> TransactionId {
        self.tx.id()
    }
}

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
