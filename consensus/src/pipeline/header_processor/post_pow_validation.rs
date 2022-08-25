use std::sync::Arc;

use consensus_core::header::Header;

use crate::errors::{BlockProcessResult, RuleError};

use super::{HeaderProcessingContext, HeaderProcessor};

impl HeaderProcessor {
    pub fn post_pow_validation(
        self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header,
    ) -> BlockProcessResult<()> {
        self.check_median_timestamp(ctx, header)
    }

    pub fn check_median_timestamp(
        self: &Arc<HeaderProcessor>, ctx: &mut HeaderProcessingContext, header: &Header,
    ) -> BlockProcessResult<()> {
        let expected_ts = self
            .past_median_time_manager
            .calc_past_median_time(ctx.ghostdag_data.clone().unwrap());

        if header.timestamp <= expected_ts {
            return Err(RuleError::ErrTimeTooOld(header.timestamp, expected_ts));
        }

        Ok(())
    }
}
