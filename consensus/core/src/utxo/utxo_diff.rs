use super::{utxo_collection::UtxoCollection, utxo_error::UtxoResult};
use crate::tx::{Transaction, TransactionOutpoint, UtxoEntry};

#[derive(Clone)]
pub struct UtxoDiff {
    pub add: UtxoCollection,
    pub remove: UtxoCollection,
}

impl UtxoDiff {
    pub fn with_diff(&self, other: &UtxoDiff) -> UtxoResult<UtxoDiff> {
        let mut clone = self.clone();
        clone.with_diff_in_place(other)?;
        Ok(clone)
    }

    pub fn with_diff_in_place(&mut self, _other: &UtxoDiff) -> UtxoResult<()> {
        todo!()
    }

    pub fn diff_from(&self, _other: &UtxoDiff) -> UtxoResult<UtxoDiff> {
        todo!()
    }

    pub fn add_transaction(&mut self, transaction: &Transaction, block_daa_score: u64) -> UtxoResult<()> {
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

    fn remove_entry(&mut self, _outpoint: &TransactionOutpoint, _entry: &UtxoEntry) -> UtxoResult<()> {
        todo!()
    }

    fn add_entry(&mut self, _outpoint: TransactionOutpoint, _entry: UtxoEntry) -> UtxoResult<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    // use super::*;

    #[test]
    fn test_types() {}
}
