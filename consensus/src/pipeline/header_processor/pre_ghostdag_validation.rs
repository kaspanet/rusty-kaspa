use super::*;
use crate::constants;
use crate::errors::{BlockProcessResult, RuleError};
use crate::model::services::reachability::ReachabilityService;
use crate::model::stores::errors::StoreResultExtensions;
use crate::model::stores::statuses::StatusesStoreReader;
use consensus_core::blockhash::BlockHashExtensions;
use consensus_core::blockstatus::BlockStatus::StatusInvalid;
use consensus_core::header::Header;
use consensus_core::BlockLevel;
use std::cmp::max;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

impl HeaderProcessor {
    pub(super) fn pre_ghostdag_validation(
        self: &Arc<HeaderProcessor>,
        ctx: &mut HeaderProcessingContext,
        header: &Header,
        is_trusted: bool,
    ) -> BlockProcessResult<()> {
        if header.hash == self.genesis_hash {
            return Ok(());
        }

        self.validate_header_in_isolation(ctx)?;
        if !is_trusted {
            self.check_parents_exist(header)?;
            self.check_parents_incest(ctx)?;
        }

        Ok(())
    }

    fn validate_header_in_isolation(self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext) -> BlockProcessResult<()> {
        if ctx.header.hash == self.genesis_hash {
            return Ok(());
        }

        self.check_header_version(ctx.header)?;
        self.check_block_timestamp_in_isolation(ctx.header)?;
        self.check_parents_limit(ctx.header)?;
        Self::check_parents_not_origin(ctx.header)?;
        self.check_pow_and_calc_block_level(ctx)?;
        Ok(())
    }

    fn check_header_version(self: &Arc<HeaderProcessor>, header: &Header) -> BlockProcessResult<()> {
        if header.version != constants::BLOCK_VERSION {
            return Err(RuleError::WrongBlockVersion(header.version));
        }
        Ok(())
    }

    fn check_block_timestamp_in_isolation(self: &Arc<HeaderProcessor>, header: &Header) -> BlockProcessResult<()> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        let max_block_time = now + self.timestamp_deviation_tolerance * self.target_time_per_block;
        if header.timestamp > max_block_time {
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
        if header.direct_parents().iter().any(|&parent| parent.is_origin()) {
            return Err(RuleError::OriginParent);
        }

        Ok(())
    }

    fn check_parents_exist(self: &Arc<HeaderProcessor>, header: &Header) -> BlockProcessResult<()> {
        let mut missing_parents = Vec::new();
        for parent in header.direct_parents() {
            match self.statuses_store.read().get(*parent).unwrap_option() {
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

    fn check_parents_incest(self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext) -> BlockProcessResult<()> {
        let parents = ctx.get_non_pruned_parents();
        for parent_a in parents.iter() {
            for parent_b in parents.iter() {
                if parent_a == parent_b {
                    continue;
                }

                if self.reachability_service.is_dag_ancestor_of(*parent_a, *parent_b) {
                    return Err(RuleError::InvalidParentsRelation(*parent_a, *parent_b));
                }
            }
        }

        Ok(())
    }

    fn check_pow_and_calc_block_level(self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext) -> BlockProcessResult<()> {
        let state = pow::State::new(ctx.header);
        let (passed, pow) = state.check_pow(ctx.header.nonce);
        if passed || self.skip_proof_of_work {
            let signed_block_level = self.max_block_level as i64 - pow.bits() as i64;
            ctx.block_level = Some(max(signed_block_level, 0) as BlockLevel);
            Ok(())
        } else {
            Err(RuleError::InvalidPoW)
        }
    }
}
