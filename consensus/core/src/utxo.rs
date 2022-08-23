use crate::tx::*;
use std::collections::HashMap;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum UtxoAlgebraError {
    #[error("outpoint {0} both in self.remove and in other.remove")]
    DuplicateRemovePoint(TransactionOutpoint),

    #[error("outpoint {0} both in self.add and in other.add")]
    DuplicateAddPoint(TransactionOutpoint),
}

pub type UtxoAlgebraResult<T> = std::result::Result<T, UtxoAlgebraError>;

pub type UtxoCollection = HashMap<TransactionOutpoint, UtxoEntry>;

#[derive(Clone)]
pub struct UtxoDiff {
    pub add: UtxoCollection,
    pub remove: UtxoCollection,
}

impl UtxoDiff {
    pub fn with_diff(&self, other: &UtxoDiff) -> UtxoAlgebraResult<UtxoDiff> {
        let mut clone = self.clone();
        clone.with_diff_in_place(other)?;
        Ok(clone)
    }

    pub fn with_diff_in_place(&mut self, other: &UtxoDiff) -> UtxoAlgebraResult<()> {
        todo!()
    }

    pub fn diff_from(&self, other: &UtxoDiff) -> UtxoAlgebraResult<UtxoDiff> {
        todo!()
    }

    pub fn add_transaction(&mut self, transaction: &Transaction, block_daa_score: u64) -> UtxoAlgebraResult<()> {
        for input in transaction.inputs.iter() {
            self.remove_entry(&input.previous_outpoint, &input.utxo_entry)?;
        }

        let is_coinbase = transaction.is_coinbase();
        let tx_id = transaction.id();

        for (i, output) in transaction.outputs.iter().enumerate() {
            let outpoint = TransactionOutpoint::new(tx_id, i as u32);
            let entry = UtxoEntry::new(output.value, output.script_public_key.clone(), block_daa_score, is_coinbase);
            self.add_entry(outpoint, entry)?;
        }
        Ok(())
    }

    fn remove_entry(&mut self, outpoint: &TransactionOutpoint, entry: &UtxoEntry) -> UtxoAlgebraResult<()> {
        todo!()
    }

    fn add_entry(&mut self, outpoint: TransactionOutpoint, entry: UtxoEntry) -> UtxoAlgebraResult<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

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
