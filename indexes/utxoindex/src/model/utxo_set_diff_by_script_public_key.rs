use consensus_core::tx::TransactionOutpoint;
use std::collections::hash_map::Iter;
use std::collections::{HashMap, HashSet};
use std::iter::Chain;

use super::*;

pub struct UtxoSetDiffByScriptPublicKey {
    pub add: UtxoSetByScriptPublicKey,
    pub remove: UtxoSetByScriptPublicKey,
}

impl UtxoSetDiffByScriptPublicKey {
    pub fn new() -> Self {
        Self { add: UtxoSetByScriptPublicKey::new(), remove: UtxoSetByScriptPublicKey::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }

    pub fn add_utxos(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }

    pub fn iter_by_outpoints_removed(&self) -> impl Iterator<Item = TransactionOutpoint> {
        //in this case we only need the outpoints
        let b = self.add.values().map(move |(v)| {
            (v.keys().next().unwrap()) //we only ever expect ond key / value per transaction outpoint.
        });
    }

    pub fn iter_by_outpoints_and_utxos_added(&self) -> impl Iterator<Item = &TransactionOutpoint> {
        let b = self.add.values().map(move |(v)| {
            (v.keys().next().unwrap(), v.values().next().unwrap()) //we only ever expect ond key / value per transaction outpoint.
        });
    }

    pub fn iter_transaction_oupoints_by_script_public_key_added(&self) {
        let a = self.remove.iter().map(move |item| {
            (item.0, HashMap::from_iter(item.1.keys())) //in this case we save a vec of outpoints per script public key.
        });
    }

    pub fn iter_transaction_oupoints_by_script_public_key_removed(&self) {
        let a = self.remove.iter().map(move |item| {
            (item.0, HashMap::from_iter(item.1.keys())) //in this case we save a vec of outpoints per script public key.
        });
    }
}
