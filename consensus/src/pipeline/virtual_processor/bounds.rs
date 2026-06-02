use std::ops::RangeInclusive;

use kaspa_smt_store::processor::SmtReadBounds;

/// Score bounds for the selected-parent -> current-block transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SeqCommitBounds {
    parent_blue_score: u64,
    #[cfg(test)]
    current_blue_score: u64,
    parent_active_min: u64,
    current_active_min: u64,
}

impl SeqCommitBounds {
    pub(super) const fn new(parent_blue_score: u64, current_blue_score: u64, inactivity_threshold: u64) -> Self {
        Self {
            parent_blue_score,
            #[cfg(test)]
            current_blue_score,
            parent_active_min: parent_blue_score.saturating_sub(inactivity_threshold),
            current_active_min: current_blue_score.saturating_sub(inactivity_threshold),
        }
    }

    #[cfg(test)]
    pub(super) fn parent_active_range(self) -> RangeInclusive<u64> {
        self.parent_active_min..=self.parent_blue_score
    }

    #[cfg(test)]
    pub(super) fn current_active_range(self) -> RangeInclusive<u64> {
        self.current_active_min..=self.current_blue_score
    }

    pub(super) const fn selected_parent_read_bounds(self) -> SmtReadBounds {
        SmtReadBounds::new(self.parent_blue_score, self.current_active_min)
    }

    /// Score band `[parent_blue_score - F, current_blue_score - F - 1]`.
    pub(super) fn newly_expired_range(self) -> Option<RangeInclusive<u64>> {
        if self.current_active_min <= self.parent_active_min {
            None
        } else {
            Some(self.parent_active_min..=self.current_active_min - 1)
        }
    }

    /// Highest blue_score that may be inclusively pruned without invading
    /// the active window at `blue_score`.
    ///
    /// The window covers `[blue_score - F, blue_score]`, so `blue_score - F`
    /// is still live and must survive pruning — the inclusive cutoff is one
    /// below it. Saturates at 0 when `blue_score <= inactivity_threshold`.
    pub(crate) const fn inclusive_prune_cutoff(blue_score: u64, inactivity_threshold: u64) -> u64 {
        blue_score.saturating_sub(inactivity_threshold).saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::SeqCommitBounds;
    use kaspa_smt_store::processor::SmtReadBounds;

    #[test]
    fn active_ranges_are_inclusive() {
        let bounds = SeqCommitBounds::new(100, 105, 10);

        assert_eq!(bounds.parent_active_range(), 90..=100);
        assert_eq!(bounds.current_active_range(), 95..=105);
    }

    #[test]
    fn newly_expired_range_returns_parent_to_current_minus_one() {
        let bounds = SeqCommitBounds::new(100, 105, 10);
        assert_eq!(bounds.newly_expired_range(), Some(90..=94));
    }

    #[test]
    fn newly_expired_range_returns_none_when_lower_cutoff_does_not_advance() {
        let same = SeqCommitBounds::new(100, 100, 10);
        let earlier = SeqCommitBounds::new(100, 99, 10);

        assert_eq!(same.newly_expired_range(), None);
        assert_eq!(earlier.newly_expired_range(), None);
    }

    #[test]
    fn selected_parent_read_bounds_use_parent_as_upper_and_current_as_lower() {
        let bounds = SeqCommitBounds::new(100, 105, 10);
        assert_eq!(bounds.selected_parent_read_bounds(), SmtReadBounds::new(100, 95));
    }

    #[test]
    fn inclusive_prune_cutoff_preserves_active_window_boundary() {
        // Active window at blue_score=100 with F=10 covers [90, 100] — score 90
        // is still live, so the inclusive prune cutoff must be 89.
        // Off-by-one regression: returning 90 would delete the boundary entry.
        assert_eq!(SeqCommitBounds::inclusive_prune_cutoff(100, 10), 89);
    }

    #[test]
    fn inclusive_prune_cutoff_saturates_below_threshold() {
        assert_eq!(SeqCommitBounds::inclusive_prune_cutoff(5, 10), 0);
        assert_eq!(SeqCommitBounds::inclusive_prune_cutoff(10, 10), 0);
        assert_eq!(SeqCommitBounds::inclusive_prune_cutoff(11, 10), 0);
        assert_eq!(SeqCommitBounds::inclusive_prune_cutoff(12, 10), 1);
    }
}
