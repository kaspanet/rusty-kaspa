use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

pub const GRAMS_PER_COMPUTE_BUDGET_UNIT: u64 = 100;
pub const GRAMS_PER_SIGOP_COUNT_UNIT: u64 = 1000;
pub const SCRIPT_UNITS_PER_GRAM: u64 = 100;
pub const SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT: u64 = GRAMS_PER_COMPUTE_BUDGET_UNIT * SCRIPT_UNITS_PER_GRAM;

/// Legacy v0 sigop-count inputs stay pegged to the historical 1000-gram sigop price.
/// So SigopCount(1) equals one actual sigop only while mass_per_sig_op == 1000;
/// that mismatch is acceptable because v0 is a deprecated compatibility path.
pub const SCRIPT_UNITS_PER_SIGOP_COUNT_UNIT: u64 = GRAMS_PER_SIGOP_COUNT_UNIT * SCRIPT_UNITS_PER_GRAM;

/// A fixed per-input script execution allowance applied before committed compute budget.
/// This is primarily intended to preserve leeway for legacy sigop-count inputs.
/// It is set to one script unit less than a single compute-budget unit, so it can cover
/// small stack work without ever buying an extra compute-budget unit or shifting incentives
/// across budget boundaries. In particular, a script requiring exactly `N` compute-budget
/// units worth of execution still needs budget `N`, because budget `N - 1` only allows
/// one script unit less than that.
#[inline(always)]
pub const fn free_script_units_per_input() -> ScriptUnits {
    ScriptUnits(SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT.saturating_sub(1))
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

impl From<SigopCount> for ScriptUnits {
    #[inline(always)]
    fn from(value: SigopCount) -> Self {
        ScriptUnits(value.0 as u64 * SCRIPT_UNITS_PER_SIGOP_COUNT_UNIT)
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

    /// Returns the smallest compute budget whose allowed script units cover the required execution.
    #[inline]
    pub fn checked_covering_script_units(required_script_units: ScriptUnits) -> Option<Self> {
        // for the current consts where free_budget = single_budget_units - 1
        // the formula below is equivalent to ordinary floor division:
        //                  required_script_units / single_budget_units
        let charged_units = required_script_units.saturating_sub(free_script_units_per_input());
        ComputeBudget::try_from(charged_units).ok()
    }
}

impl From<u16> for ComputeBudget {
    #[inline(always)]
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl TryFrom<ScriptUnits> for ComputeBudget {
    type Error = std::num::TryFromIntError;

    #[inline]
    fn try_from(units: ScriptUnits) -> Result<Self, Self::Error> {
        let scaled = units.0.div_ceil(SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT);
        Ok(Self(u16::try_from(scaled)?))
    }
}

impl From<ComputeBudget> for ScriptUnits {
    #[inline(always)]
    fn from(value: ComputeBudget) -> Self {
        ScriptUnits(value.0 as u64 * SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT)
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

impl From<Gram> for ScriptUnits {
    #[inline(always)]
    fn from(value: Gram) -> Self {
        Self(value.0 * SCRIPT_UNITS_PER_GRAM)
    }
}

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[serde(transparent)]
pub struct ScriptUnits(pub u64);

impl ScriptUnits {
    pub const fn saturating_add(self, other: ScriptUnits) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    pub const fn saturating_sub(self, other: ScriptUnits) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    pub fn checked_sub(self, other: ScriptUnits) -> Option<Self> {
        self.0.checked_sub(other.0).map(Self)
    }
}

impl std::ops::Add for ScriptUnits {
    type Output = Self;

    #[inline(always)]
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl std::ops::Sub for ScriptUnits {
    type Output = Self;

    #[inline(always)]
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
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

#[cfg(test)]
mod tests {
    use crate::tx::TxInputMass;

    use super::{ComputeBudget, ScriptUnits};

    #[test]
    fn checked_covering_script_units_respects_free_allowance_boundaries() {
        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(0)), Some(ComputeBudget(0)));
        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(9999)), Some(ComputeBudget(0)));
        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(10_000)), Some(ComputeBudget(1)));
        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(19_999)), Some(ComputeBudget(1)));
        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(20_000)), Some(ComputeBudget(2)));
    }

    #[test]
    fn checked_covering_script_units_returns_none_on_budget_overflow() {
        let max_coverable = TxInputMass::from(ComputeBudget(u16::MAX)).allowed_script_units();

        assert_eq!(ComputeBudget::checked_covering_script_units(max_coverable), Some(ComputeBudget(u16::MAX)));
        assert_eq!(ComputeBudget::checked_covering_script_units(max_coverable.saturating_add(ScriptUnits(1))), None);
    }
}
