use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

pub const GRAMS_PER_COMPUTE_BUDGET_UNIT: u64 = 10;
pub const SCRIPT_UNITS_PER_GRAM: u64 = 10;
pub const SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT: u64 = GRAMS_PER_COMPUTE_BUDGET_UNIT * SCRIPT_UNITS_PER_GRAM;
/// A fixed per-input execution allowance applied before any committed compute budget.
/// Before the covenants hard fork, script execution was primarily bounded by fixed engine limits,
/// so version-0 transactions did not need to explicitly budget for stack work. After lifting
/// several of those limits and introducing stack pricing, legacy version-0 wallets still commit
/// only a sigop count. This free allowance keeps such transactions valid as long as they perform
/// only minimal stack work. The allowance is set just below the cost of one additional sigop.
#[inline(always)]
pub const fn free_script_units_per_input(sigop_script_units: u64) -> ScriptUnits {
    ScriptUnits(sigop_script_units.saturating_sub(SCRIPT_UNITS_PER_GRAM))
}

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

    #[inline(always)]
    pub const fn to_grams(self) -> Gram {
        Gram(self.0 as u64 * GRAMS_PER_COMPUTE_BUDGET_UNIT)
    }

    #[inline(always)]
    pub const fn to_script_units(self) -> ScriptUnits {
        ScriptUnits(self.0 as u64 * SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT)
    }

    #[inline(always)]
    pub const fn allowed_script_units(self, sigop_script_units: u64) -> ScriptUnits {
        ScriptUnits(self.to_script_units().value() + free_script_units_per_input(sigop_script_units).value())
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

    #[inline(always)]
    pub const fn to_script_units(self) -> ScriptUnits {
        ScriptUnits(self.0 * SCRIPT_UNITS_PER_GRAM)
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
