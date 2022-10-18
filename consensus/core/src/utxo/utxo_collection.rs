use super::utxo_view::UtxoView;
use crate::tx::*;
use std::collections::HashMap;

pub type UtxoCollection = HashMap<TransactionOutpoint, UtxoEntry>;

pub trait UtxoCollectionExtensions {
    /// Checks if the `outpoint` key exists with an entry that holds `entry.block_daa_score == daa_score`
    fn contains_with_daa_score(&self, outpoint: &TransactionOutpoint, daa_score: u64) -> bool;

    /// Adds all entries from `other` to `self`.
    /// Note that this means that values from `other` might override values of `self`.
    fn add_collection(&mut self, other: &Self);

    /// Removes all elements in `other` from `self`. Equivalent to `self - other` in set theory.
    fn remove_collection(&mut self, other: &Self);

    /// Returns whether the intersection between the two collections is not empty.
    fn intersects(&self, other: &Self) -> bool;

    /// Checks if there is an intersection between two utxo collections satisfying an arbitrary `rule`.
    /// Returns the first outpoint in such an intersection, or `None` if the intersection is empty.
    fn intersects_with_rule<F>(&self, other: &Self, rule: F) -> Option<TransactionOutpoint>
    where
        F: Fn(&TransactionOutpoint, &UtxoEntry, &UtxoEntry) -> bool;
}

impl UtxoView for UtxoCollection {
    fn get(&self, outpoint: &TransactionOutpoint) -> Option<UtxoEntry> {
        self.get(outpoint).cloned()
    }
}

impl UtxoCollectionExtensions for UtxoCollection {
    fn contains_with_daa_score(&self, outpoint: &TransactionOutpoint, daa_score: u64) -> bool {
        if let Some(entry) = self.get(outpoint) {
            entry.block_daa_score == daa_score
        } else {
            false
        }
    }

    fn add_collection(&mut self, other: &Self) {
        for (k, v) in other.iter() {
            self.insert(*k, v.clone());
        }
    }

    fn remove_collection(&mut self, other: &Self) {
        for k in other.keys() {
            self.remove(k);
        }
    }

    fn intersects(&self, other: &Self) -> bool {
        // We prefer iterating over the smaller set
        let (keys, other) = if self.len() <= other.len() { (self.keys(), other) } else { (other.keys(), self) };

        for k in keys {
            if other.contains_key(k) {
                return true;
            }
        }
        false
    }

    fn intersects_with_rule<F>(&self, other: &Self, rule: F) -> Option<TransactionOutpoint>
    where
        F: Fn(&TransactionOutpoint, &UtxoEntry, &UtxoEntry) -> bool,
    {
        // We prefer iterating over the smaller set
        if self.len() <= other.len() {
            for (k, v1) in self.iter() {
                if let Some(v2) = other.get(k) {
                    if rule(k, v1, v2) {
                        return Some(*k);
                    }
                }
            }
        } else {
            for (k, v2) in other.iter() {
                if let Some(v1) = self.get(k) {
                    // Note we always make sure to call the rule in the correct order
                    if rule(k, v1, v2) {
                        return Some(*k);
                    }
                }
            }
        }
        None
    }
}

//
// Functions for UTXO diff algebra with daa score dimension considerations
//

/// Calculates the intersection and subtraction between two utxo collections.
/// The function returns with the following outcome:
///
/// `result    = result ∪ (this ∩ other)`
///
/// `remainder = remainder ∪ (this ∖ other)`
///
/// where the set operators demand equality also on the DAA score dimension
pub(super) fn intersection_with_remainder_having_daa_score_in_place(
    this: &UtxoCollection,
    other: &UtxoCollection,
    result: &mut UtxoCollection,
    remainder: &mut UtxoCollection,
) {
    for (outpoint, entry) in this.iter() {
        if other.contains_with_daa_score(outpoint, entry.block_daa_score) {
            result.insert(*outpoint, entry.clone());
        } else {
            remainder.insert(*outpoint, entry.clone());
        }
    }
}

/// Calculates the subtraction between two utxo collections.
/// The function returns with the following outcome:
///
/// `result    = result ∪ (this ∖ other)`
///
/// where the set operators demand equality also on the DAA score dimension
pub(super) fn subtraction_having_daa_score_in_place(this: &UtxoCollection, other: &UtxoCollection, result: &mut UtxoCollection) {
    for (outpoint, entry) in this.iter() {
        if !other.contains_with_daa_score(outpoint, entry.block_daa_score) {
            result.insert(*outpoint, entry.clone());
        }
    }
}

/// Calculates the subtraction and intersection between two utxo collections.
/// The function returns with the following outcome:
///
/// `result    = result ∪ (this ∖ other)`
///
/// `remainder = remainder ∪ (this ∩ other)`
///
/// where the set operators demand equality also on the DAA score dimension
pub(super) fn subtraction_with_remainder_having_daa_score_in_place(
    this: &UtxoCollection,
    other: &UtxoCollection,
    result: &mut UtxoCollection,
    remainder: &mut UtxoCollection,
) {
    for (outpoint, entry) in this.iter() {
        if !other.contains_with_daa_score(outpoint, entry.block_daa_score) {
            result.insert(*outpoint, entry.clone());
        } else {
            remainder.insert(*outpoint, entry.clone());
        }
    }
}
