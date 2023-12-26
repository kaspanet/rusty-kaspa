use super::{
    utxo_collection::*,
    utxo_error::{UtxoAlgebraError, UtxoResult},
};
use crate::tx::{TransactionOutpoint, UtxoEntry, VerifiableTransaction};
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};
use std::{collections::hash_map::Entry::Vacant, mem::size_of};

pub trait ImmutableUtxoDiff {
    fn added(&self) -> &UtxoCollection;
    fn removed(&self) -> &UtxoCollection;
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtxoDiff {
    pub add: UtxoCollection,
    pub remove: UtxoCollection,
}

impl MemSizeEstimator for UtxoDiff {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>() + (self.add.len() + self.remove.len()) * (size_of::<TransactionOutpoint>() + size_of::<UtxoEntry>())
    }
}

impl<T: ImmutableUtxoDiff> ImmutableUtxoDiff for &T {
    fn added(&self) -> &UtxoCollection {
        (*self).added()
    }
    fn removed(&self) -> &UtxoCollection {
        (*self).removed()
    }
}

impl ImmutableUtxoDiff for UtxoDiff {
    fn added(&self) -> &UtxoCollection {
        &self.add
    }

    fn removed(&self) -> &UtxoCollection {
        &self.remove
    }
}

pub struct ReversedUtxoDiff<'a> {
    inner: &'a UtxoDiff,
}

impl<'a> ReversedUtxoDiff<'a> {
    pub fn new(inner: &'a UtxoDiff) -> Self {
        Self { inner }
    }
}

impl ImmutableUtxoDiff for ReversedUtxoDiff<'_> {
    fn added(&self) -> &UtxoCollection {
        &self.inner.remove // Reverse inner
    }

    fn removed(&self) -> &UtxoCollection {
        &self.inner.add // Reverse inner
    }
}

impl UtxoDiff {
    pub fn new(add: UtxoCollection, remove: UtxoCollection) -> Self {
        Self { add, remove }
    }

    pub fn as_reversed(&self) -> impl ImmutableUtxoDiff + '_ {
        ReversedUtxoDiff::new(self)
    }

    pub fn to_reversed(self) -> Self {
        Self::new(self.remove, self.add)
    }

    pub fn with_diff(&self, other: &impl ImmutableUtxoDiff) -> UtxoResult<UtxoDiff> {
        let mut clone = self.clone();
        clone.with_diff_in_place(other)?;
        Ok(clone)
    }

    /// Applies the provided diff to this diff in-place. This is equal to if the
    /// first diff, and then the second diff were applied to the same base UTXO set
    pub fn with_diff_in_place(&mut self, other: &impl ImmutableUtxoDiff) -> UtxoResult<()> {
        // TODO: should we apply the sanity checks below only in Debug mode?
        if let Some(offending_outpoint) =
            other.removed().intersects_with_rule(&self.remove, |outpoint, entry_to_add, _existing_entry| {
                !self.add.contains_with_daa_score(outpoint, entry_to_add.block_daa_score)
            })
        {
            return Err(UtxoAlgebraError::DuplicateRemovePoint(offending_outpoint));
        }

        if let Some(offending_outpoint) = other.added().intersects_with_rule(&self.add, |outpoint, _entry_to_add, existing_entry| {
            !other.removed().contains_with_daa_score(outpoint, existing_entry.block_daa_score)
        }) {
            return Err(UtxoAlgebraError::DuplicateAddPoint(offending_outpoint));
        }

        let mut intersection = UtxoCollection::new();

        // If does not exist neither in `add` nor in `remove` - add to `remove`
        intersection_with_remainder_having_daa_score_in_place(other.removed(), &self.add, &mut intersection, &mut self.remove);
        // If already exists in `add` with the same DAA score - remove from `add`
        self.add.remove_collection(&intersection);

        intersection.clear();

        // If does not exist neither in `add` nor in `remove`, or exists in `remove' with different DAA score - add to 'add'
        intersection_with_remainder_having_daa_score_in_place(other.added(), &self.remove, &mut intersection, &mut self.add);
        // If already exists in `remove` with the same DAA score - remove from `remove`
        self.remove.remove_collection(&intersection);

        Ok(())
    }

    /// Returns a new UTXO diff with the difference between this diff and another
    /// Assumes that:
    /// Both diffs are from the same base
    /// If an outpoint exists in both diffs, its underlying values would be the same
    ///
    /// diff_from follows a set of rules represented by the following 3 by 3 table:
    ///
    /// ```text
    ///          |           |   this    |           |
    /// ---------+-----------+-----------+-----------+-----------
    ///          |           |   add     |   remove  |   None
    /// ---------+-----------+-----------+-----------+-----------
    /// other    |   add     |   -       |   X       |   add
    /// ---------+-----------+-----------+-----------+-----------
    ///          |   remove  |   X       |   -       |   remove
    /// ---------+-----------+-----------+-----------+-----------
    ///          |   None    |   remove  |   add     |   -
    ///
    /// Key:
    /// -         Don't add anything to the result
    /// X         Return an error
    /// add       Add the UTXO into the add collection of the result
    /// remove    Add the UTXO into the remove collection of the result
    /// ```
    ///
    /// Examples:
    /// 1. This diff contains a UTXO in add, and the other diff contains it in remove
    ///    diffFrom results in an error
    /// 2. This diff contains a UTXO in remove, and the other diff does not contain it
    ///    diffFrom results in the UTXO being added to add
    pub fn diff_from(&self, other: &impl ImmutableUtxoDiff) -> UtxoResult<UtxoDiff> {
        // Note that the following cases are not accounted for, as they are impossible
        // as long as the base UTXO set is the same:
        // - if utxo entry is in this.add and other.remove
        // - if utxo entry is in this.remove and other.add

        // TODO: make sure the table above is up-to-date with regard to the DAA score dimension.
        // TODO: should we apply the sanity checks below only in Debug mode?

        // Check that NOT(entries with unequal DAA scores AND utxo is in self.add and/or other.remove) -> Error
        let rule_not_added_output_removed_with_daa_score =
            |outpoint: &TransactionOutpoint, this_entry: &UtxoEntry, other_entry: &UtxoEntry| {
                !(other_entry.block_daa_score != this_entry.block_daa_score
                    && (self.add.contains_with_daa_score(outpoint, other_entry.block_daa_score)
                        || other.removed().contains_with_daa_score(outpoint, this_entry.block_daa_score)))
            };

        if let Some(offending_outpoint) = self.remove.intersects_with_rule(other.added(), rule_not_added_output_removed_with_daa_score)
        {
            return Err(UtxoAlgebraError::DiffIntersectionPoint(offending_outpoint, "both in self.add and in other.remove"));
        }

        // Check that NOT(entries with unequal DAA score AND utxo is in self.remove and/or other.add) -> Error
        let rule_not_removed_output_added_with_daa_score =
            |outpoint: &TransactionOutpoint, this_entry: &UtxoEntry, other_entry: &UtxoEntry| {
                !(other_entry.block_daa_score != this_entry.block_daa_score
                    && (self.remove.contains_with_daa_score(outpoint, other_entry.block_daa_score)
                        || other.added().contains_with_daa_score(outpoint, this_entry.block_daa_score)))
            };

        if let Some(offending_outpoint) = self.add.intersects_with_rule(other.removed(), rule_not_removed_output_added_with_daa_score)
        {
            return Err(UtxoAlgebraError::DiffIntersectionPoint(offending_outpoint, "both in self.remove and in other.add"));
        }

        // If we have the same entry in self.remove and other.remove
        // and existing entry is with different DAA score -> Error
        if let Some(offending_outpoint) = self.remove.intersects_with_rule(other.removed(), |_outpoint, this_entry, other_entry| {
            other_entry.block_daa_score != this_entry.block_daa_score
        }) {
            return Err(UtxoAlgebraError::DiffIntersectionPoint(
                offending_outpoint,
                "both in self.remove and other.remove with different DAA scores, with no corresponding entry in self.add",
            ));
        }

        let mut result = UtxoDiff::default();

        // All utxos in self.add:
        // If they are not in other.add - should be added in result.remove
        let mut in_both_to_add = UtxoCollection::new();
        subtraction_with_remainder_having_daa_score_in_place(&self.add, other.added(), &mut result.remove, &mut in_both_to_add);
        // If they are in other.remove - base utxo-set is not the same
        if in_both_to_add.intersects(&self.remove) != in_both_to_add.intersects(other.removed()) {
            return Err(UtxoAlgebraError::General(
                "diff_from: outpoint both in self.add, other.add, and only one of self.remove and other.remove",
            ));
        }

        // All utxos in other.remove:
        // If they are not in self.remove - should be added in result.remove
        subtraction_having_daa_score_in_place(other.removed(), &self.remove, &mut result.remove);

        // All utxos in self.remove:
        // If they are not in other.remove - should be added in result.add
        subtraction_having_daa_score_in_place(&self.remove, other.removed(), &mut result.add);

        // All utxos in other.add:
        // If they are not in self.add - should be added in result.add
        subtraction_having_daa_score_in_place(other.added(), &self.add, &mut result.add);

        Ok(result)
    }

    pub fn add_transaction(&mut self, transaction: &impl VerifiableTransaction, block_daa_score: u64) -> UtxoResult<()> {
        for (input, entry) in transaction.populated_inputs() {
            self.remove_entry(&input.previous_outpoint, entry)?;
        }

        let is_coinbase = transaction.is_coinbase();
        let tx_id = transaction.id();

        for (i, output) in transaction.outputs().iter().enumerate() {
            let outpoint = TransactionOutpoint::new(tx_id, i as u32);
            let entry = UtxoEntry::new(output.value, output.script_public_key.clone(), block_daa_score, is_coinbase);
            self.add_entry(outpoint, entry)?;
        }
        Ok(())
    }

    fn remove_entry(&mut self, outpoint: &TransactionOutpoint, entry: &UtxoEntry) -> UtxoResult<()> {
        if self.add.contains_with_daa_score(outpoint, entry.block_daa_score) {
            self.add.remove(outpoint);
        } else if let Vacant(e) = self.remove.entry(*outpoint) {
            e.insert(entry.clone());
        } else {
            return Err(UtxoAlgebraError::DoubleRemoveCall(*outpoint));
        }
        Ok(())
    }

    fn add_entry(&mut self, outpoint: TransactionOutpoint, entry: UtxoEntry) -> UtxoResult<()> {
        if self.remove.contains_with_daa_score(&outpoint, entry.block_daa_score) {
            self.remove.remove(&outpoint);
        } else if let Vacant(e) = self.add.entry(outpoint) {
            e.insert(entry);
        } else {
            return Err(UtxoAlgebraError::DoubleAddCall(outpoint));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{ScriptPublicKey, TransactionId};
    use std::str::FromStr;

    #[test]
    fn test_utxo_diff_rules() {
        let tx_id0 = TransactionId::from_str("0".repeat(64).as_str()).unwrap();
        let outpoint0 = TransactionOutpoint::new(tx_id0, 0);
        let utxo_entry1 = UtxoEntry::new(10, ScriptPublicKey::default(), 0, true);
        let utxo_entry2 = UtxoEntry::new(20, ScriptPublicKey::default(), 1, true);

        struct Test {
            name: &'static str,
            this: UtxoDiff,
            other: UtxoDiff,
            expected_diff_from_result: UtxoResult<UtxoDiff>,
            expected_with_diff_result: UtxoResult<UtxoDiff>,
        }

        struct UtxoDiffBuilder {
            diff: UtxoDiff,
        }

        impl UtxoDiffBuilder {
            fn new() -> Self {
                Self { diff: UtxoDiff::default() }
            }

            fn insert_add_point(mut self, outpoint: TransactionOutpoint, entry: UtxoEntry) -> Self {
                assert!(self.diff.add.insert(outpoint, entry).is_none());
                self
            }

            fn insert_remove_point(mut self, outpoint: TransactionOutpoint, entry: UtxoEntry) -> Self {
                assert!(self.diff.remove.insert(outpoint, entry).is_none());
                self
            }

            fn build(self) -> UtxoDiff {
                self.diff
            }
        }

        let tests = [
            Test {
                name: "first add in this, first add in other",
                this: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new().build()),
                expected_with_diff_result: Err(UtxoAlgebraError::DuplicateAddPoint(outpoint0)),
            },
            Test {
                name: "first add in this, second add in other",
                this: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry2.clone()).build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry2.clone())
                    .insert_remove_point(outpoint0, utxo_entry1.clone())
                    .build()),
                expected_with_diff_result: Err(UtxoAlgebraError::DuplicateAddPoint(outpoint0)),
            },
            Test {
                name: "first add in this, second remove in other",
                this: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build(),
                expected_diff_from_result: Err(UtxoAlgebraError::DiffIntersectionPoint(outpoint0, "")),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new().build()),
            },
            Test {
                name: "first add in this and other, second remove in other",
                this: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build(),
                expected_diff_from_result: Err(UtxoAlgebraError::General("")),
                expected_with_diff_result: Err(UtxoAlgebraError::DuplicateAddPoint(outpoint0)),
            },
            Test {
                name: "first add in this and remove in other, second add in other",
                this: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry2.clone())
                    .insert_remove_point(outpoint0, utxo_entry1.clone())
                    .build(),
                expected_diff_from_result: Err(UtxoAlgebraError::DiffIntersectionPoint(outpoint0, "")),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry2.clone()).build()),
            },
            Test {
                name: "first add in this, empty other",
                this: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new().build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build()),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build()),
            },
            Test {
                name: "first remove in this and add in other",
                this: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build(),
                expected_diff_from_result: Err(UtxoAlgebraError::DiffIntersectionPoint(outpoint0, "")),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new().build()),
            },
            Test {
                name: "first remove in this, second add in other",
                this: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry2.clone()).build(),
                expected_diff_from_result: Err(UtxoAlgebraError::DiffIntersectionPoint(outpoint0, "")),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry2.clone())
                    .insert_remove_point(outpoint0, utxo_entry1.clone())
                    .build()),
            },
            Test {
                name: "first remove in this and other",
                this: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new().build()),
                expected_with_diff_result: Err(UtxoAlgebraError::DuplicateRemovePoint(outpoint0)),
            },
            Test {
                name: "first remove in this, second remove in other",
                this: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry2.clone()).build(),
                expected_diff_from_result: Err(UtxoAlgebraError::DiffIntersectionPoint(outpoint0, "")),
                expected_with_diff_result: Err(UtxoAlgebraError::DuplicateRemovePoint(outpoint0)),
            },
            Test {
                name: "first remove in this and add in other, second remove in other",
                this: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build(),
                expected_diff_from_result: Err(UtxoAlgebraError::DiffIntersectionPoint(outpoint0, "")),
                expected_with_diff_result: Err(UtxoAlgebraError::DuplicateRemovePoint(outpoint0)),
            },
            Test {
                name: "first remove in this and other, second add in other",
                this: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry2.clone())
                    .insert_remove_point(outpoint0, utxo_entry1.clone())
                    .build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry2.clone()).build()),
                expected_with_diff_result: Err(UtxoAlgebraError::DuplicateRemovePoint(outpoint0)),
            },
            Test {
                name: "first remove in this, empty other",
                this: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build(),
                other: UtxoDiffBuilder::new().build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build()),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build()),
            },
            Test {
                name: "first add in this and other, second remove in this",
                this: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build(),
                other: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build(),
                expected_diff_from_result: Err(UtxoAlgebraError::General("")),
                expected_with_diff_result: Err(UtxoAlgebraError::DuplicateAddPoint(outpoint0)),
            },
            Test {
                name: "first add in this, second remove in this and add in other",
                this: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build(),
                other: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry2.clone()).build(),
                expected_diff_from_result: Err(UtxoAlgebraError::DiffIntersectionPoint(outpoint0, "")),
                expected_with_diff_result: Err(UtxoAlgebraError::DuplicateAddPoint(outpoint0)),
            },
            Test {
                name: "first add in this and remove in other, second remove in this",
                this: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build(),
                other: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build(),
                expected_diff_from_result: Err(UtxoAlgebraError::DiffIntersectionPoint(outpoint0, "")),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry2.clone()).build()),
            },
            Test {
                name: "first add in this, second remove in this and in other",
                this: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build(),
                other: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry2.clone()).build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build()),
                expected_with_diff_result: Err(UtxoAlgebraError::DuplicateRemovePoint(outpoint0)),
            },
            Test {
                name: "first add and second remove in both this and other",
                this: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build(),
                other: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new().build()),
                expected_with_diff_result: Err(UtxoAlgebraError::DuplicateRemovePoint(outpoint0)),
            },
            Test {
                name: "first add in this and remove in other, second remove in this and add in other",
                this: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build(),
                other: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry2.clone())
                    .insert_remove_point(outpoint0, utxo_entry1.clone())
                    .build(),
                expected_diff_from_result: Err(UtxoAlgebraError::DiffIntersectionPoint(outpoint0, "")),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new().build()),
            },
            Test {
                name: "first add and second remove in this, empty other",
                this: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build(),
                other: UtxoDiffBuilder::new().build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry2.clone())
                    .insert_remove_point(outpoint0, utxo_entry1.clone())
                    .build()),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build()),
            },
            Test {
                name: "empty this, first add in other",
                this: UtxoDiffBuilder::new().build(),
                other: UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build()),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new().insert_add_point(outpoint0, utxo_entry1.clone()).build()),
            },
            Test {
                name: "empty this, first remove in other",
                this: UtxoDiffBuilder::new().build(),
                other: UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build()),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new().insert_remove_point(outpoint0, utxo_entry1.clone()).build()),
            },
            Test {
                name: "empty this, first add and second remove in other",
                this: UtxoDiffBuilder::new().build(),
                other: UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build()),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new()
                    .insert_add_point(outpoint0, utxo_entry1.clone())
                    .insert_remove_point(outpoint0, utxo_entry2.clone())
                    .build()),
            },
            Test {
                name: "empty this, empty other",
                this: UtxoDiffBuilder::new().build(),
                other: UtxoDiffBuilder::new().build(),
                expected_diff_from_result: Ok(UtxoDiffBuilder::new().build()),
                expected_with_diff_result: Ok(UtxoDiffBuilder::new().build()),
            },
        ];

        // Run the tests
        for test in tests {
            let diff_from_result = test.this.diff_from(&test.other);
            assert_eq!(diff_from_result, test.expected_diff_from_result, "diff_from failed for test \"{}\"", test.name);

            if let Ok(diff_from_inner) = diff_from_result {
                assert_eq!(
                    test.this.with_diff(&diff_from_inner).unwrap(),
                    test.other,
                    "reverse diff_from failed for test \"{}\"",
                    test.name
                );
            }

            let with_diff_result = test.this.with_diff(&test.other);
            assert_eq!(with_diff_result, test.expected_with_diff_result, "with_diff failed for test \"{}\"", test.name);

            if let Ok(with_diff_inner) = with_diff_result {
                assert_eq!(
                    test.this.diff_from(&with_diff_inner).unwrap(),
                    test.other,
                    "reverse with_diff failed for test \"{}\"",
                    test.name
                );
            }
        }

        // Avoid compiler warnings on the last clone
        drop(utxo_entry1);
        drop(utxo_entry2);
    }
}
