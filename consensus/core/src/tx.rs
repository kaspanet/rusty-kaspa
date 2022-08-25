use crate::{
    hashing,
    subnets::{self, SubnetworkId},
};
use std::{cell::RefCell, fmt::Display, sync::Arc};

/// Represents the ID of a Kaspa transaction
pub type TransactionId = hashes::Hash;

/// Represents a Kaspad ScriptPublicKey
#[derive(Default, Debug)]
pub struct ScriptPublicKey {
    pub script: Vec<u8>,
    pub version: u16,
}

impl ScriptPublicKey {
    pub fn new(script: Vec<u8>, version: u16) -> Self {
        Self { script, version }
    }
}

/// Houses details about an individual transaction output in a utxo
/// set such as whether or not it was contained in a coinbase tx, the daa
/// score of the block that accepts the tx, its public key script, and how
/// much it pays.
#[derive(Debug, Clone)]
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

/// Represents a Kaspa transaction outpoint
#[derive(Eq, Hash, PartialEq, Debug, Clone)]
pub struct TransactionOutpoint {
    pub transaction_id: TransactionId,
    pub index: u32,
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
pub struct TransactionInput {
    pub previous_outpoint: TransactionOutpoint,
    pub signature_script: Vec<u8>,
    pub sequence: u64,
    pub sig_op_count: u8,
    pub utxo_entry: UtxoEntry,
}

impl TransactionInput {
    pub fn new(
        previous_outpoint: TransactionOutpoint, signature_script: Vec<u8>, sequence: u64, sig_op_count: u8,
        utxo_entry: UtxoEntry,
    ) -> Self {
        Self { previous_outpoint, signature_script, sequence, sig_op_count, utxo_entry }
    }
}

/// Represents a Kaspad transaction output
pub struct TransactionOutput {
    pub value: u64,
    pub script_public_key: Arc<ScriptPublicKey>,
}

/// Represents a Kaspa transaction
pub struct Transaction {
    pub version: u16,
    pub inputs: Vec<Arc<TransactionInput>>,
    pub outputs: Vec<Arc<TransactionOutput>>,
    pub lock_time: u64,
    pub subnetwork_id: SubnetworkId,
    pub gas: u64,
    pub payload: Vec<u8>,

    pub fee: u64,
    pub mass: u64,

    // A field that is used to cache the transaction ID.
    // Always use the corresponding self.id() instead of accessing this field directly
    cached_id: RefCell<Option<TransactionId>>, // TODO: see how should be adapted for multi-threading
}

impl Transaction {
    pub fn new(
        version: u16, inputs: Vec<Arc<TransactionInput>>, outputs: Vec<Arc<TransactionOutput>>, lock_time: u64,
        subnetwork_id: SubnetworkId, gas: u64, payload: Vec<u8>, fee: u64, mass: u64,
    ) -> Self {
        Self {
            version,
            inputs,
            outputs,
            lock_time,
            subnetwork_id,
            gas,
            payload,
            fee,
            mass,
            cached_id: RefCell::new(None),
        }
    }

    /// Determines whether or not a transaction is a coinbase transaction. A coinbase
    /// transaction is a special transaction created by miners that distributes fees and block subsidy
    /// to the previous blocks' miners, and to specify the scriptPubKey that will be used to pay the current
    /// miner in future blocks. Each input of the coinbase transaction should set index to maximum
    /// value and reference the relevant block id, instead of previous transaction id.
    pub fn is_coinbase(&self) -> bool {
        self.subnetwork_id == subnets::SUBNETWORK_ID_COINBASE
    }

    pub fn id(&self) -> TransactionId {
        // This method should probably be implemented in a thread-safe manner,
        // however for single-thread usage the usage of RefCell is perfectly
        // fine since we borrow_mut exactly once

        if let Some(id) = *self.cached_id.borrow() {
            return id;
        }

        let mut op = self.cached_id.borrow_mut();
        let id = hashing::tx::transaction_id(self);
        *op = Some(id);
        id
    }
}

#[cfg(test)]
mod tests {
    // use super::*;

    #[test]
    fn test_types() {}
}
