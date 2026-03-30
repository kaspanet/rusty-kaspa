use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[serde(transparent)]
pub struct SigopCount(pub u8);

impl SigopCount {
    #[inline(always)]
    pub const fn value(self) -> u8 {
        self.0
    }
}

impl From<u8> for SigopCount {
    #[inline(always)]
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<SigopCount> for u8 {
    #[inline(always)]
    fn from(value: SigopCount) -> Self {
        value.0
    }
}

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[serde(transparent)]
pub struct ComputeBudget(pub u16);

impl ComputeBudget {
    #[inline(always)]
    pub const fn value(self) -> u16 {
        self.0
    }
}

impl From<u16> for ComputeBudget {
    #[inline(always)]
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<ComputeBudget> for u16 {
    #[inline(always)]
    fn from(value: ComputeBudget) -> Self {
        value.0
    }
}

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[serde(transparent)]
pub struct Gram(pub u64);

impl Gram {
    #[inline(always)]
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for Gram {
    #[inline(always)]
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<Gram> for u64 {
    #[inline(always)]
    fn from(value: Gram) -> Self {
        value.0
    }
}

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[serde(transparent)]
pub struct ScriptUnits(pub u64);

impl ScriptUnits {
    #[inline(always)]
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for ScriptUnits {
    #[inline(always)]
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<ScriptUnits> for u64 {
    #[inline(always)]
    fn from(value: ScriptUnits) -> Self {
        value.0
    }
}
