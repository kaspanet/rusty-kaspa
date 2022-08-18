use super::*;
use crate::constants;
use crate::errors::{BlockProcessResult, RuleError};
use consensus_core::header::Header;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

impl HeaderProcessor {
    pub(super) fn validate_header_in_isolation(self: &Arc<HeaderProcessor>, header: &Header) -> BlockProcessResult<()> {
        if header.hash == self.genesis_hash {
            return Ok(());
        }

        self.check_header_version(header)?;
        self.check_block_timestamp_in_isolation(header)?;
        self.check_parents_limit(header)?;
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
        if header.parents.is_empty() {
            return Err(RuleError::NoParents);
        }

        if header.parents.len() > self.max_block_parents as usize {
            return Err(RuleError::TooManyParents(header.parents.len(), self.max_block_parents as usize));
        }

        Ok(())
    }
}
