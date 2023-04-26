use super::*;
use crate::errors::{BlockProcessResult, RuleError};
use crate::model::services::reachability::ReachabilityService;
use kaspa_consensus_core::header::Header;
use std::sync::Arc;

impl HeaderProcessor {
    pub(super) fn pre_pow_validation(
        self: &Arc<HeaderProcessor>,
        ctx: &mut HeaderProcessingContext,
        header: &Header,
    ) -> BlockProcessResult<()> {
        if header.hash == self.genesis.hash {
            return Ok(());
        }

        self.check_pruning_violation(ctx)?;
        self.check_difficulty_and_daa_score(ctx, header)?;
        Ok(())
    }

    fn check_pruning_violation(self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext) -> BlockProcessResult<()> {
        let non_pruned_parents = ctx.get_non_pruned_parents();
        if non_pruned_parents.is_empty() {
            return Ok(());
        }

        // We check that the new block is in the future of the pruning point by verifying that at least
        // one of its parents is in the pruning point future (or the pruning point itself). Otherwise,
        // the Prunality proof implies that the block can be discarded.
        if !self.reachability_service.is_dag_ancestor_of_any(ctx.pruning_point(), &mut non_pruned_parents.iter().copied()) {
            return Err(RuleError::PruningViolation(ctx.pruning_point()));
        }
        Ok(())
    }

    fn check_difficulty_and_daa_score(
        self: &Arc<HeaderProcessor>,
        ctx: &mut HeaderProcessingContext,
        header: &Header,
    ) -> BlockProcessResult<()> {
        let ghostdag_data = ctx.get_ghostdag_data().unwrap();
        let window = self.dag_traversal_manager.block_window(&ghostdag_data, self.difficulty_window_size)?;

        let (daa_score, mergeset_non_daa) = self
            .difficulty_manager
            .calc_daa_score_and_non_daa_mergeset_blocks(&mut window.iter().map(|item| item.0.hash), &ghostdag_data);

        if daa_score != header.daa_score {
            return Err(RuleError::UnexpectedHeaderDaaScore(daa_score, header.daa_score));
        }

        ctx.mergeset_non_daa = mergeset_non_daa;

        let expected_bits = self.difficulty_manager.calculate_difficulty_bits(&window);
        if header.bits != expected_bits {
            return Err(RuleError::UnexpectedDifficulty(header.bits, expected_bits));
        }

        ctx.block_window_for_difficulty = Some(window);
        Ok(())
    }
}
