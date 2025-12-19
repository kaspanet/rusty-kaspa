use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

use crate::{RpcError, RpcResult};

const NONE: &str = "None";
const LOW: &str = "Low";
const MEDIUM: &str = "Medium";
const HIGH: &str = "High";
const FULL: &str = "Full";

#[derive(PartialEq, PartialOrd, Eq, Default, Clone, Copy, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum RpcVerbosityTiers {
    #[default]
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Full = 4,
}

impl RpcVerbosityTiers {
    #[inline(always)]
    pub fn is_none(self) -> bool {
        self == RpcVerbosityTiers::None
    }

    #[inline(always)]
    pub fn is_none_or_higher(self) -> bool {
        true // by necessity, as `None` is the lowest tier
    }

    #[inline(always)]
    pub fn is_low_or_higher(self) -> bool {
        self == RpcVerbosityTiers::Low || self.is_medium_or_higher()
    }

    #[inline(always)]
    pub fn is_medium_or_higher(self) -> bool {
        self == RpcVerbosityTiers::Medium || self.is_high_or_higher()
    }

    #[inline(always)]
    pub fn is_high_or_higher(self) -> bool {
        self == RpcVerbosityTiers::High || self.is_full()
    }

    #[inline(always)]
    pub fn is_full(self) -> bool {
        self == RpcVerbosityTiers::Full
    }
}

impl TryFrom<String> for RpcVerbosityTiers {
    type Error = RpcError;

    #[inline(always)]
    fn try_from(value: String) -> RpcResult<Self> {
        Ok(match value.as_str() {
            NONE => RpcVerbosityTiers::None,
            LOW => RpcVerbosityTiers::Low,
            MEDIUM => RpcVerbosityTiers::Medium,
            HIGH => RpcVerbosityTiers::High,
            FULL => RpcVerbosityTiers::Full,
            _ => return Err(RpcError::ParseStringError(value)),
        })
    }
}

impl ToString for RpcVerbosityTiers {
    #[inline(always)]
    fn to_string(&self) -> String {
        match self {
            RpcVerbosityTiers::None => NONE.to_string(),
            RpcVerbosityTiers::Low => LOW.to_string(),
            RpcVerbosityTiers::Medium => MEDIUM.to_string(),
            RpcVerbosityTiers::High => HIGH.to_string(),
            RpcVerbosityTiers::Full => FULL.to_string(),
        }
    }
}
