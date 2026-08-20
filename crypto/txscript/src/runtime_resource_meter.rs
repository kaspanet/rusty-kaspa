use kaspa_consensus_core::mass::ScriptUnits;
use kaspa_txscript_errors::TxScriptError;

/// Tracks resource consumption (signature operations and pushed bytes, both expressed
/// as script units) during script execution against a committed budget.
#[derive(Debug, Clone)]
pub struct RuntimeResourceMeter {
    used_sig_ops: u16,
    sigop_script_units: ScriptUnits,
    script_units_limit: ScriptUnits,
    remaining_script_units: ScriptUnits,
}

impl RuntimeResourceMeter {
    pub fn new_script_units(sigop_script_units: ScriptUnits, script_units_limit: ScriptUnits) -> Self {
        Self { used_sig_ops: 0, sigop_script_units, script_units_limit, remaining_script_units: script_units_limit }
    }

    pub fn used_sig_ops(&self) -> u16 {
        self.used_sig_ops
    }

    pub fn used_script_units(&self) -> ScriptUnits {
        self.script_units_limit - self.remaining_script_units
    }

    pub fn consume_script_units(&mut self, units: ScriptUnits) -> Result<(), TxScriptError> {
        match self.remaining_script_units.checked_sub(units) {
            Some(new_remaining) => {
                self.remaining_script_units = new_remaining;
                Ok(())
            }
            None => {
                let overflow = units.0 - self.remaining_script_units.0;
                let used_units = self.script_units_limit.0.saturating_add(overflow);
                Err(TxScriptError::ExceededCommittedScriptUnits { used: used_units, limit: self.script_units_limit.0 })
            }
        }
    }

    pub fn consume_sig_op_cost(&mut self, count: u16) -> Result<(), TxScriptError> {
        self.consume_script_units(ScriptUnits((count as u64).saturating_mul(self.sigop_script_units.0)))?;
        self.used_sig_ops = self.used_sig_ops.saturating_add(count);
        Ok(())
    }

    pub fn charge_newly_pushed_bytes(&mut self, pushed_bytes_delta: u64) -> Result<(), TxScriptError> {
        // Pushed bytes are charged 1:1 as script units.
        self.consume_script_units(pushed_bytes_delta.into())
    }
}

#[cfg(test)]
mod tests {
    use kaspa_core::assert_match;

    use super::*;

    #[test]
    fn script_units_meter_charges_sigops_in_script_units() {
        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(100), ScriptUnits(250));

        assert_eq!(meter.consume_sig_op_cost(2), Ok(()));
        assert_eq!(meter.used_sig_ops(), 2);
        assert_eq!(meter.used_script_units(), ScriptUnits(200));

        assert_eq!(meter.consume_sig_op_cost(1), Err(TxScriptError::ExceededCommittedScriptUnits { used: 300, limit: 250 }));
        assert_eq!(meter.used_sig_ops(), 2);
        assert_eq!(meter.used_script_units(), ScriptUnits(200));
    }

    #[test]
    fn script_units_meter_saturates_exceeded_used_units() {
        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(100));

        assert_eq!(meter.consume_script_units(ScriptUnits(60)), Ok(()));
        assert_eq!(
            meter.consume_script_units(ScriptUnits(u64::MAX)),
            Err(TxScriptError::ExceededCommittedScriptUnits { used: u64::MAX, limit: 100 })
        );
        assert_eq!(meter.used_script_units(), ScriptUnits(60));
    }

    #[test]
    fn script_units_meter_rejects_u64_max_charge_without_panicking() {
        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(100));

        assert_eq!(
            meter.consume_script_units(ScriptUnits(u64::MAX)),
            Err(TxScriptError::ExceededCommittedScriptUnits { used: u64::MAX, limit: 100 })
        );
        assert_eq!(meter.used_script_units(), ScriptUnits(0));
    }

    #[test]
    fn script_units_meter_charges_only_newly_pushed_bytes() {
        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(20));

        assert_eq!(meter.charge_newly_pushed_bytes(7), Ok(()));
        assert_eq!(meter.used_script_units(), ScriptUnits(7));

        // Charging zero is a no-op because only newly pushed bytes are charged.
        assert_eq!(meter.charge_newly_pushed_bytes(0), Ok(()));
        assert_eq!(meter.used_script_units(), ScriptUnits(7));

        assert_eq!(meter.charge_newly_pushed_bytes(9), Ok(()));
        assert_eq!(meter.used_script_units(), ScriptUnits(16));
    }

    #[test]
    fn meter_bounds_do_not_panic() {
        let mut max_used_sig_ops_meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(0));
        assert_eq!(max_used_sig_ops_meter.consume_sig_op_cost(u16::MAX), Ok(()));
        assert_eq!(max_used_sig_ops_meter.used_sig_ops(), u16::MAX);
        assert_eq!(max_used_sig_ops_meter.consume_sig_op_cost(1), Ok(()));
        assert_eq!(max_used_sig_ops_meter.used_sig_ops(), u16::MAX);

        let mut max_units_meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(u64::MAX));
        assert_eq!(max_units_meter.charge_newly_pushed_bytes(u64::MAX), Ok(()));
        assert_eq!(max_units_meter.used_script_units(), ScriptUnits(u64::MAX));
        assert_match!(
            max_units_meter.charge_newly_pushed_bytes(u64::MAX),
            Err(TxScriptError::ExceededCommittedScriptUnits { used: _, limit: _ })
        ); // Charging u64::MAX again should excceed budget and not panic due to overflow.
        assert_eq!(max_units_meter.used_script_units(), ScriptUnits(u64::MAX)); // On overflow, used_script_units are not affected.

        // Now checking overflow behaviour when starting non-max u64 values.
        let mut max_units_meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(u64::MAX));
        assert_eq!(max_units_meter.charge_newly_pushed_bytes(u64::MAX - 1), Ok(()));
        assert_eq!(max_units_meter.used_script_units(), ScriptUnits(u64::MAX - 1));
        assert_match!(
            max_units_meter.charge_newly_pushed_bytes(2),
            Err(TxScriptError::ExceededCommittedScriptUnits { used: _, limit: _ })
        ); // Charging 2 should excceed budget and not panic due to overflow.
        assert_eq!(max_units_meter.used_script_units(), ScriptUnits(u64::MAX - 1)); // On overflow, used_script_units are not affected.
    }
}
