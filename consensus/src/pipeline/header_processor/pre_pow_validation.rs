use super::*;
use crate::constants;
use crate::errors::{BlockProcessResult, RuleError};
use crate::model::services::reachability::ReachabilityService;
use crate::model::stores::errors::StoreResultExtensions;
use crate::model::stores::pruning::PruningStoreReader;
use crate::model::stores::statuses::{BlockStatus::StatusInvalid, StatusesStoreReader};
use consensus_core::blockhash::BlockHashExtensions;
use consensus_core::header::Header;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

impl HeaderProcessor {
    pub(super) fn pre_pow_validation(
        self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header,
    ) -> BlockProcessResult<()> {
        if header.hash == self.genesis_hash {
            return Ok(());
        }

        self.validate_header_in_isolation(header)?;
        self.check_parents_exist(header)?;
        self.check_parents_incest(ctx, header)?;
        self.check_pruning_violation(ctx, header)?;
        self.check_pow(ctx, header)?;
        self.check_difficulty_and_daa_score(ctx, header)?;
        Ok(())
    }

    fn validate_header_in_isolation(self: &Arc<HeaderProcessor>, header: &Header) -> BlockProcessResult<()> {
        if header.hash == self.genesis_hash {
            return Ok(());
        }

        self.check_header_version(header)?;
        self.check_block_timestamp_in_isolation(header)?;
        self.check_parents_limit(header)?;
        Self::check_parents_not_origin(header)?;
        Ok(())
    }

    fn check_header_version(self: &Arc<HeaderProcessor>, header: &Header) -> BlockProcessResult<()> {
        if header.version != constants::BLOCK_VERSION {
            return Err(RuleError::WrongBlockVersion(header.version));
        }
        Ok(())
    }

    fn check_block_timestamp_in_isolation(self: &Arc<HeaderProcessor>, header: &Header) -> BlockProcessResult<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let max_block_time = now + self.timestamp_deviation_tolerance * self.target_time_per_block;
        if header.timestamp > now {
            return Err(RuleError::TimeTooFarIntoTheFuture(header.timestamp, now));
        }
        Ok(())
    }

    fn check_parents_limit(self: &Arc<HeaderProcessor>, header: &Header) -> BlockProcessResult<()> {
        if header.direct_parents().is_empty() {
            return Err(RuleError::NoParents);
        }

        if header.direct_parents().len() > self.max_block_parents as usize {
            return Err(RuleError::TooManyParents(header.direct_parents().len(), self.max_block_parents as usize));
        }

        Ok(())
    }

    fn check_parents_not_origin(header: &Header) -> BlockProcessResult<()> {
        if header
            .direct_parents()
            .iter()
            .any(|&parent| parent.is_origin())
        {
            return Err(RuleError::OriginParent);
        }

        Ok(())
    }

    fn check_parents_exist(self: &Arc<HeaderProcessor>, header: &Header) -> BlockProcessResult<()> {
        let mut missing_parents = Vec::new();
        for parent in header.direct_parents() {
            match self
                .statuses_store
                .read()
                .get(*parent)
                .unwrap_option()
            {
                None => missing_parents.push(*parent),
                Some(StatusInvalid) => {
                    return Err(RuleError::InvalidParent(*parent));
                }
                Some(_) => {}
            }
        }
        if !missing_parents.is_empty() {
            return Err(RuleError::MissingParents(missing_parents));
        }
        Ok(())
    }

    fn check_parents_incest(
        self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header,
    ) -> BlockProcessResult<()> {
        let parents = ctx.get_non_pruned_parents();
        for parent_a in parents.iter() {
            for parent_b in parents.iter() {
                if parent_a == parent_b {
                    continue;
                }

                if self
                    .reachability_service
                    .is_dag_ancestor_of(*parent_a, *parent_b)
                {
                    return Err(RuleError::InvalidParentsRelation(*parent_a, *parent_b));
                }
            }
        }

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
            .calculate_difficulty_bits(&mut window.iter().map(|item| item.0.hash));
        if header.bits != expected_bits {
            return Err(RuleError::UnexpectedDifficulty(header.bits, expected_bits));
        }

        ctx.block_window_for_difficulty = Some(window);
        Ok(())
    }
}
