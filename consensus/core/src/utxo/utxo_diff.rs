use super::{
    utxo_collection::{
        subtraction_having_daa_score_in_place, subtraction_with_remainder_having_daa_score_in_place, UtxoCollection,
        UtxoCollectionExtensions,
    },
    utxo_error::{UtxoAlgebraError, UtxoResult},
};
use crate::tx::{Transaction, TransactionOutpoint, UtxoEntry};

#[derive(Clone, Default)]
pub struct UtxoDiff {
    pub add: UtxoCollection,
    pub remove: UtxoCollection,
}

impl UtxoDiff {
    pub fn new() -> Self {
        Self { add: Default::default(), remove: Default::default() }
    }

    pub fn with_diff(&self, other: &UtxoDiff) -> UtxoResult<UtxoDiff> {
        let mut clone = self.clone();
        clone.with_diff_in_place(other)?;
        Ok(clone)
    }

    pub fn with_diff_in_place(&mut self, _other: &UtxoDiff) -> UtxoResult<()> {
        todo!()
    }

    pub fn diff_from(&self, other: &UtxoDiff) -> UtxoResult<UtxoDiff> {
        // Check that NOT (entries with unequal DAA scores AND utxo is in self.add and/or other.remove) -> Error
        let rule_not_added_output_removed_with_daa_score =
            |outpoint: &TransactionOutpoint, this_entry: &UtxoEntry, other_entry: &UtxoEntry| {
                !(other_entry.block_daa_score != this_entry.block_daa_score
                    && (self
                        .add
                        .contains_with_daa_score(outpoint, other_entry.block_daa_score)
                        || other
                            .remove
                            .contains_with_daa_score(outpoint, this_entry.block_daa_score)))
            };

        if let Some(offending_outpoint) = self
            .remove
            .intersects_with_rule(&other.add, rule_not_added_output_removed_with_daa_score)
        {
            return Err(UtxoAlgebraError::DiffIntersectionPoint(
                offending_outpoint,
                "both in self.add and in other.remove",
            ));
        }

        // Check that NOT (entries with unequal DAA score AND utxo is in self.remove and/or other.add) -> Error
        let rule_not_removed_output_added_with_daa_score =
            |outpoint: &TransactionOutpoint, this_entry: &UtxoEntry, other_entry: &UtxoEntry| {
                !(other_entry.block_daa_score != this_entry.block_daa_score
                    && (self
                        .remove
                        .contains_with_daa_score(outpoint, other_entry.block_daa_score)
                        || other
                            .add
                            .contains_with_daa_score(outpoint, this_entry.block_daa_score)))
            };

        if let Some(offending_outpoint) = self
            .add
            .intersects_with_rule(&other.remove, rule_not_removed_output_added_with_daa_score)
        {
            return Err(UtxoAlgebraError::DiffIntersectionPoint(
                offending_outpoint,
                "both in self.remove and in other.add",
            ));
        }

        // If we have the same entry in self.remove and other.remove
        // and existing entry is with different DAA score -> Error
        if let Some(offending_outpoint) = self.remove.intersects_with_rule(
            &other.remove,
            |_outpoint: &TransactionOutpoint, this_entry: &UtxoEntry, other_entry: &UtxoEntry| {
                other_entry.block_daa_score != this_entry.block_daa_score
            },
        ) {
            return Err(UtxoAlgebraError::DiffIntersectionPoint(
                offending_outpoint,
                "both in self.remove and other.remove with different DAA scores, with no corresponding entry in self.add",
            ));
        }

        let mut result = UtxoDiff::new();

        // All transactions in self.add:
        // If they are not in other.add - should be added in result.remove
        let mut in_both_to_add = UtxoCollection::new();
        subtraction_with_remainder_having_daa_score_in_place(
            &self.add,
            &other.add,
            &mut result.remove,
            &mut in_both_to_add,
        );
        // If they are in other.remove - base utxo-set is not the same
        if in_both_to_add.intersects(&self.remove) != in_both_to_add.intersects(&other.remove) {
            return Err(UtxoAlgebraError::General(
                "diff_from: outpoint both in self.add, other.add, and only one of self.remove and other.remove",
            ));
        }

        // All transactions in other.remove:
        // If they are not in self.remove - should be added in result.remove
        subtraction_having_daa_score_in_place(&other.remove, &self.remove, &mut result.remove);

        // All transactions in self.remove:
        // If they are not in other.remove - should be added in result.add
        subtraction_having_daa_score_in_place(&self.remove, &other.remove, &mut result.add);

        // All transactions in other.add:
        // If they are not in self.add - should be added in result.add
        subtraction_having_daa_score_in_place(&other.add, &self.add, &mut result.add);

        Ok(result)
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
