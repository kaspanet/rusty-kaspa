use serde::{Deserialize, Serialize};

use crate::{
    hashing,
    subnets::{self, SubnetworkId},
};
use std::{fmt::Display, sync::Arc};

/// Represents the ID of a Kaspa transaction
pub type TransactionId = hashes::Hash;

/// Represents a Kaspad ScriptPublicKey
#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct ScriptPublicKey {
    pub script: Vec<u8>,
    pub version: u16,
}

impl ScriptPublicKey {
    pub fn new(script: Vec<u8>, version: u16) -> Self {
        Self { script, version }
    }
}

/// Holds details about an individual transaction output in a utxo
/// set such as whether or not it was contained in a coinbase tx, the daa
/// score of the block that accepts the tx, its public key script, and how
/// much it pays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtxoEntry {
    pub amount: u64,
    pub script_public_key: Arc<ScriptPublicKey>,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl UtxoEntry {
    pub fn new(amount: u64, script_public_key: Arc<ScriptPublicKey>, block_daa_score: u64, is_coinbase: bool) -> Self {
        Self { amount, script_public_key, block_daa_score, is_coinbase }
    }
}

pub type TransactionIndexType = u32;

/// Represents a Kaspa transaction outpoint
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize, Clone)] // TODO: Implement a custom serializer for input that drops utxo_entry
pub struct TransactionInput {
    pub previous_outpoint: TransactionOutpoint,
    pub signature_script: Vec<u8>,
    pub sequence: u64,
    pub sig_op_count: u8,
}

impl TransactionInput {
    pub fn new(previous_outpoint: TransactionOutpoint, signature_script: Vec<u8>, sequence: u64, sig_op_count: u8) -> Self {
        Self { previous_outpoint, signature_script, sequence, sig_op_count }
    }
}

/// Represents a Kaspad transaction output
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionOutput {
    pub value: u64,
    pub script_public_key: Arc<ScriptPublicKey>,
}

impl TransactionOutput {
    pub fn new(value: u64, script_public_key: Arc<ScriptPublicKey>) -> Self {
        Self { value, script_public_key }
    }
}

/// Represents a Kaspa transaction
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Transaction {
    pub version: u16,
    pub inputs: Vec<Arc<TransactionInput>>,   // TODO: arcs make no sense here
    pub outputs: Vec<Arc<TransactionOutput>>, // TODO: arcs make no sense here
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
        inputs: Vec<Arc<TransactionInput>>,
        outputs: Vec<Arc<TransactionOutput>>,
        lock_time: u64,
        subnetwork_id: SubnetworkId,
        gas: u64,
        payload: Vec<u8>,
        _fee: u64, // TODO: remove
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
        self.tx.inputs.iter().map(std::ops::Deref::deref).zip(self.entries.iter())
    }

    pub fn outputs(&self) -> &[Arc<TransactionOutput>] {
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
        self.tx.inputs.iter().map(std::ops::Deref::deref).zip(self.entries.iter())
    }

    pub fn outputs(&self) -> &[Arc<TransactionOutput>] {
        &self.tx.outputs
    }

    pub fn is_coinbase(&self) -> bool {
        self.tx.is_coinbase()
    }

    pub fn id(&self) -> TransactionId {
        self.tx.id()
    }
}
