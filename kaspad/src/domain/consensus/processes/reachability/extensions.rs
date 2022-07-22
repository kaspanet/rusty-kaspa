use crate::domain::consensus::model::{
    api::hash::DomainHash,
    stores::{errors::StoreError, reachability::ReachabilityStore},
};

use super::interval::Interval;

pub(super) type StoreResult<T> = std::result::Result<T, StoreError>;

impl dyn ReachabilityStore + '_ {
    pub(super) fn interval_children_capacity(&self, block: &DomainHash) -> StoreResult<Interval> {
        // We subtract 1 from the end of the range to prevent the node from allocating
        // the entire interval to its children, since we want the interval to *strictly*
        // contain the intervals of its children.
        Ok(self.get_interval(block)?.decrease_end(1))
    }

    pub(super) fn remaining_interval_before(&self, block: &DomainHash) -> StoreResult<Interval> {
        let children_capacity = self.interval_children_capacity(block)?;
        match self.get_children(block)?.first() {
            Some(first_child) => {
                let first_child_interval = self.get_interval(first_child)?;
                Ok(Interval::new(children_capacity.start, first_child_interval.start - 1))
            }
            None => Ok(children_capacity),
        }
    }

    pub(super) fn remaining_interval_after(&self, block: &DomainHash) -> StoreResult<Interval> {
        let children_capacity = self.interval_children_capacity(block)?;
        match self.get_children(block)?.last() {
            Some(last_child) => {
                let last_child_interval = self.get_interval(last_child)?;
                Ok(Interval::new(last_child_interval.end + 1, children_capacity.end))
            }
            None => Ok(children_capacity),
        }
    }
}
