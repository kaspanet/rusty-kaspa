use std::{collections::HashMap, sync::Arc};

/// The size of the array used to store subnetwork IDs.
pub const SUBNETWORK_ID_SIZE: usize = 20;

/// The domain representation of a Subnetwork ID
pub type SubnetworkId = [u8; SUBNETWORK_ID_SIZE];

/// Represents the ID of a Kaspa transaction
pub type TransactionId = hashes::Hash;

/// Represents a Kaspad ScriptPublicKey
#[derive(Default, Debug)]
pub struct ScriptPublicKey {
    pub script: Vec<u8>,
    pub version: u16,
}

/// Houses details about an individual transaction output in a utxo
/// set such as whether or not it was contained in a coinbase tx, the daa
/// score of the block that accepts the tx, its public key script, and how
/// much it pays.
#[derive(Debug)]
pub struct UtxoEntry {
    pub amount: u64,
    pub script_public_key: Arc<ScriptPublicKey>,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

/// Represents a Kaspa transaction outpoint
#[derive(Eq, Hash, PartialEq, Debug)]
pub struct TransactionOutpoint {
    pub transaction_id: TransactionId,
    pub index: u32,
}

/// Represents a Kaspa transaction input
pub struct TransactionInput {
    pub previous_outpoint: TransactionOutpoint,
    pub signature_script: Vec<u8>,
    pub sequence: u64,
    pub sig_op_count: u8,
    pub utxo_entry: UtxoEntry,
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
    pub outpoints: Vec<Arc<TransactionOutput>>,
    pub lock_time: u64,
    pub subnetwork_id: SubnetworkId,
    pub gas: u64,
    pub payload: Vec<u8>,

    pub fee: u64,
    pub mass: u64,
    // A field that is used to cache the transaction ID.
    // Always use consensushashing.TransactionId instead of accessing this field directly
    // pub id: Option<TransactionId>, // TODO: see how should be rusted
}

pub type UtxoCollection = HashMap<TransactionOutpoint, UtxoEntry>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_types() {
        let mut map = UtxoCollection::new();
        map.insert(
            TransactionOutpoint { transaction_id: 6.into(), index: 1 },
            UtxoEntry {
                amount: 5,
                script_public_key: Arc::new(ScriptPublicKey::default()),
                block_daa_score: 765,
                is_coinbase: false,
            },
        );
        dbg!(map);
    }
}
