use super::*;
use crate::errors::{BlockProcessResult, RuleError};
use crate::model::services::reachability::ReachabilityService;
use kaspa_consensus_core::header::Header;

impl HeaderProcessor {
    pub(super) fn pre_pow_validation(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        self.check_pruning_violation(ctx)?;
        self.check_difficulty_and_daa_score(ctx, header)?;
        Ok(())
    }

    fn check_pruning_violation(&self, ctx: &HeaderProcessingContext) -> BlockProcessResult<()> {
        let known_parents = ctx.direct_known_parents();

        // We check that the new block is in the future of the pruning point by verifying that at least
        // one of its parents is in the pruning point future (or the pruning point itself). Otherwise,
        // the Prunality proof implies that the block can be discarded.
        if !self.reachability_service.is_dag_ancestor_of_any(ctx.pruning_point(), &mut known_parents.iter().copied()) {
            return Err(RuleError::PruningViolation(ctx.pruning_point()));
        }
        Ok(())
    }

    fn check_difficulty_and_daa_score(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        let ghostdag_data = ctx.ghostdag_data();
        let window = self.dag_traversal_manager.block_window(ghostdag_data, self.difficulty_window_size)?;

        let (daa_score, mergeset_non_daa) = self.difficulty_manager.calc_daa_score_and_non_daa_mergeset_blocks(
            &window,
            ghostdag_data,
            self.ghostdag_stores[0].as_ref(),
        );

        if daa_score != header.daa_score {
            return Err(RuleError::UnexpectedHeaderDaaScore(daa_score, header.daa_score));
        }

        ctx.mergeset_non_daa = Some(mergeset_non_daa);

        let expected_bits = self.difficulty_manager.calculate_difficulty_bits(&window);
        if header.bits != expected_bits {
            return Err(RuleError::UnexpectedDifficulty(header.bits, expected_bits));
        }

        ctx.block_window_for_difficulty = Some(window);
        Ok(())
    }
}
