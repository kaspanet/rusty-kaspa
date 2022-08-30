use std::sync::Arc;

use consensus_core::header::Header;

use crate::errors::{BlockProcessResult, RuleError};

use super::{HeaderProcessingContext, HeaderProcessor};

impl HeaderProcessor {
    pub fn post_pow_validation(
        self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header,
    ) -> BlockProcessResult<()> {
        self.check_median_timestamp(ctx, header)?;
        self.check_merge_size_limit(ctx, header)
    }

    pub fn check_median_timestamp(
        self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header,
    ) -> BlockProcessResult<()> {
        let (expected_ts, window) = self
            .past_median_time_manager
            .calc_past_median_time(ctx.ghostdag_data.clone().unwrap());
        ctx.block_window_for_past_median_time = Some(window);

        if header.timestamp <= expected_ts {
            return Err(RuleError::TimeTooOld(header.timestamp, expected_ts));
        }

        Ok(())
    }

    pub fn check_merge_size_limit(
        self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header,
    ) -> BlockProcessResult<()> {
        let mergeset_size = ctx
            .ghostdag_data
            .as_ref()
            .unwrap()
            .mergeset_size() as u64;

        if mergeset_size > self.mergeset_size_limit {
            return Err(RuleError::MergeSetTooBig(mergeset_size, self.mergeset_size_limit));
        }
        Ok(())
    }
}
