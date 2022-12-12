use super::{HeaderProcessingContext, HeaderProcessor};
use crate::errors::{BlockProcessResult, RuleError, TwoDimVecDisplay};
use crate::model::services::reachability::ReachabilityService;
use consensus_core::header::Header;
use hashes::Hash;
use std::collections::HashSet;
use std::sync::Arc;

impl HeaderProcessor {
    pub fn post_pow_validation(
        self: &Arc<HeaderProcessor>,
        ctx: &mut HeaderProcessingContext,
        header: &Header,
    ) -> BlockProcessResult<()> {
        self.check_blue_score(ctx, header)?;
        self.check_blue_work(ctx, header)?;
        self.check_median_timestamp(ctx, header)?;
        self.check_merge_size_limit(ctx)?;
        self.check_bounded_merge_depth(ctx)?;
        self.check_pruning_point(ctx, header)?;
        self.check_indirect_parents(ctx, header)
    }

    pub fn check_median_timestamp(
        self: &Arc<HeaderProcessor>,
        ctx: &mut HeaderProcessingContext,
        header: &Header,
    ) -> BlockProcessResult<()> {
        let (past_median_time, window) = self.past_median_time_manager.calc_past_median_time(&ctx.ghostdag_data.clone().unwrap());
        ctx.block_window_for_past_median_time = Some(window);

        if header.timestamp <= past_median_time {
            return Err(RuleError::TimeTooOld(header.timestamp, past_median_time));
        }

        Ok(())
    }

    pub fn check_merge_size_limit(self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext) -> BlockProcessResult<()> {
        let mergeset_size = ctx.ghostdag_data.as_ref().unwrap().mergeset_size() as u64;

        if mergeset_size > self.mergeset_size_limit {
            return Err(RuleError::MergeSetTooBig(mergeset_size, self.mergeset_size_limit));
        }
        Ok(())
    }

    fn check_blue_score(self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        let gd_blue_score = ctx.ghostdag_data.as_ref().unwrap().blue_score;
        if gd_blue_score != header.blue_score {
            return Err(RuleError::UnexpectedHeaderBlueScore(gd_blue_score, header.blue_score));
        }
        Ok(())
    }

    fn check_blue_work(self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        let gd_blue_work = ctx.ghostdag_data.as_ref().unwrap().blue_work;
        if gd_blue_work != header.blue_work {
            return Err(RuleError::UnexpectedHeaderBlueWork(gd_blue_work, header.blue_work));
        }
        Ok(())
    }

    pub fn check_indirect_parents(
        self: &Arc<HeaderProcessor>,
        ctx: &mut HeaderProcessingContext,
        header: &Header,
    ) -> BlockProcessResult<()> {
        let expected_block_parents = self.parents_manager.calc_block_parents(ctx.pruning_point(), header.direct_parents());
        if header.parents_by_level.len() != expected_block_parents.len()
            || !expected_block_parents.iter().enumerate().all(|(block_level, expected_level_parents)| {
                let header_level_parents = &header.parents_by_level[block_level];
                if header_level_parents.len() != expected_level_parents.len() {
                    return false;
                }

                let expected_set = HashSet::<&Hash>::from_iter(expected_level_parents);
                header_level_parents.iter().all(|header_parent| expected_set.contains(header_parent))
            })
        {
            return Err(RuleError::UnexpectedIndirectParents(
                TwoDimVecDisplay(expected_block_parents),
                TwoDimVecDisplay(header.parents_by_level.clone()),
            ));
        };
        Ok(())
    }

    pub fn check_pruning_point(
        self: &Arc<HeaderProcessor>,
        ctx: &mut HeaderProcessingContext,
        header: &Header,
    ) -> BlockProcessResult<()> {
        let expected =
            self.pruning_manager.expected_header_pruning_point(ctx.ghostdag_data.as_ref().unwrap().to_compact(), ctx.pruning_info);
        if expected != header.pruning_point {
            return Err(RuleError::WrongHeaderPruningPoint(expected, header.pruning_point));
        }
        Ok(())
    }

    pub fn check_bounded_merge_depth(self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext) -> BlockProcessResult<()> {
        let gd_data = ctx.ghostdag_data.as_ref().unwrap();
        let merge_depth_root = self.depth_manager.calc_merge_depth_root(gd_data, ctx.pruning_point());
        let finality_point = self.depth_manager.calc_finality_point(gd_data, ctx.pruning_point());
        let mut kosherizing_blues: Option<Vec<Hash>> = None;

        for red in gd_data.mergeset_reds.iter().copied() {
            if self.reachability_service.is_dag_ancestor_of(merge_depth_root, red) {
                continue;
            }
            // Lazy load the kosherizing blocks since this case is extremely rare
            if kosherizing_blues.is_none() {
                kosherizing_blues = Some(self.depth_manager.kosherizing_blues(gd_data, merge_depth_root).collect());
            }
            if !self.reachability_service.is_dag_ancestor_of_any(red, &mut kosherizing_blues.as_ref().unwrap().iter().copied()) {
                return Err(RuleError::ViolatingBoundedMergeDepth);
            }
        }

        ctx.merge_depth_root = Some(merge_depth_root);
        ctx.finality_point = Some(finality_point);
        Ok(())
    }
}
