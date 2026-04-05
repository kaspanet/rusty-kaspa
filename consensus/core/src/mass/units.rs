use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

pub const GRAMS_PER_COMPUTE_BUDGET_UNIT: u64 = 100;
pub const SCRIPT_UNITS_PER_GRAM: u64 = 10;
pub const SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT: u64 = GRAMS_PER_COMPUTE_BUDGET_UNIT * SCRIPT_UNITS_PER_GRAM;

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
    pub const fn allowed_script_units(self) -> ScriptUnits {
        ScriptUnits(self.to_script_units().value() + free_script_units_per_input().value())
    }

    /// Returns the smallest compute budget whose allowed script units cover the required execution.
    #[inline]
    pub fn checked_covering_script_units(required_script_units: ScriptUnits) -> Option<Self> {
        // for the current consts where free_budget = single_budget_units - 1
        // the formula below is equivalent to ordinary floor division:
        //                  required_script_units / single_budget_units
        let charged_units = required_script_units.value().saturating_sub(free_script_units_per_input().value());
        let budget_units = charged_units.div_ceil(SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT);
        u16::try_from(budget_units).ok().map(Self)
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

#[cfg(test)]
mod tests {
    use super::{ComputeBudget, ScriptUnits};

    #[test]
    fn checked_covering_script_units_respects_free_allowance_boundaries() {
        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(0)), Some(ComputeBudget(0)));
        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(999)), Some(ComputeBudget(0)));
        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(1_000)), Some(ComputeBudget(1)));
        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(1_999)), Some(ComputeBudget(1)));
        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(2_000)), Some(ComputeBudget(2)));
    }

    #[test]
    fn checked_covering_script_units_returns_none_on_budget_overflow() {
        let max_coverable = ComputeBudget(u16::MAX).allowed_script_units().value();

        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(max_coverable)), Some(ComputeBudget(u16::MAX)));
        assert_eq!(ComputeBudget::checked_covering_script_units(ScriptUnits(max_coverable + 1)), None);
    }
}
