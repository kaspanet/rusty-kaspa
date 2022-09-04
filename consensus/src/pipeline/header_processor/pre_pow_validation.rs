use super::*;
use crate::errors::{BlockProcessResult, RuleError};
use crate::model::services::reachability::ReachabilityService;
use crate::model::stores::errors::StoreResultExtensions;
use crate::model::stores::pruning::PruningStoreReader;
use consensus_core::header::Header;
use std::sync::Arc;

impl HeaderProcessor {
    pub(super) fn pre_pow_validation(
        self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header,
    ) -> BlockProcessResult<()> {
        if header.hash == self.genesis_hash {
            return Ok(());
        }

        self.check_pruning_violation(ctx, header)?;
        self.check_pow(ctx, header)?;
        self.check_difficulty_and_daa_score(ctx, header)?;
        Ok(())
    }

    fn check_pruning_violation(
        self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header,
    ) -> BlockProcessResult<()> {
        match self
            .pruning_store
            .read()
            .pruning_point()
            .unwrap_option()
        {
            None => Ok(()), // It implictly means that genesis is the pruning point - so no violation can exist
            Some(pruning_point) => {
                let non_pruned_parents = ctx.get_non_pruned_parents();
                if non_pruned_parents.is_empty() {
                    return Ok(());
                }

                if non_pruned_parents.iter().cloned().any(|parent| {
                    !self
                        .reachability_service
                        .is_dag_ancestor_of(pruning_point, parent)
                }) {
                    return Err(RuleError::PruningViolation(pruning_point));
                }

                Ok(())
            }
        }
    }

    fn check_pow(
        self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header,
    ) -> BlockProcessResult<()> {
        Ok(()) // TODO: Check PoW
    }

    fn check_difficulty_and_daa_score(
        self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header,
    ) -> BlockProcessResult<()> {
        let ghostdag_data = ctx.ghostdag_data.clone().unwrap();
        let window = self
            .dag_traversal_manager
            .block_window(ghostdag_data, self.difficulty_window_size);

        let (daa_score, daa_added_blocks) = self
            .difficulty_manager
            .calc_daa_score_and_added_blocks(
                &mut window.iter().map(|item| item.0.hash),
                &ctx.ghostdag_data.clone().unwrap(),
            );

        if daa_score != header.daa_score {
            return Err(RuleError::UnexpectedHeaderDaaScore(daa_score, header.daa_score));
        }

        ctx.daa_added_blocks = Some(daa_added_blocks);

        let expected_bits = self
            .difficulty_manager
            .calculate_difficulty_bits(&window);
        // TODO: Uncomment once DAA calculation is right
        if header.bits != expected_bits {
            return Err(RuleError::UnexpectedDifficulty(header.bits, expected_bits));
        }

        ctx.block_window_for_difficulty = Some(window);
        Ok(())
    }
}
