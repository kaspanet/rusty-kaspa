use super::*;
use crate::constants;
use crate::errors::{ConsensusError, ConsensusResult, RuleError};
use consensus_core::header::Header;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

impl HeaderProcessor {
    pub(super) fn validate_header_in_isolation(self: &Arc<HeaderProcessor>, header: &Header) -> ConsensusResult<()> {
        if header.hash == self.genesis_hash {
            return Ok(());
        }

        self.check_header_version(header)?;
        Ok(())
    }

    fn check_header_version(self: &Arc<HeaderProcessor>, header: &Header) -> ConsensusResult<()> {
        if header.version != constants::BLOCK_VERSION {
            return Err(ConsensusError::RuleError(RuleError::WrongBlockVersion(header.version)));
        }
        Ok(())
    }

    fn check_block_timestamp_in_isolation(self: &Arc<HeaderProcessor>, header: &Header) -> ConsensusResult<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let max_block_time = now + self.timestamp_deviation_tolerance * self.target_time_per_block;
        if header.time_in_ms > now {
            return Err(ConsensusError::RuleError(RuleError::TimeTooMuchInTheFuture(header.time_in_ms, now)));
        }
        Ok(())
    }

    fn check_parents_limit(self: &Arc<HeaderProcessor>, header: &Header) -> ConsensusResult<()> {
        if header.parents.len() == 0 {
            return Err(ConsensusError::RuleError(RuleError::NoParents));
        }

        if header.parents.len() as u8 > self.max_block_parents {
            return Err(ConsensusError::RuleError(RuleError::TooManyParents(header.parents.len() as u64)));
        }

        Ok(())
    }
}
