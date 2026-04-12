use kaspa_consensus_core::mass::ScriptUnits;
use kaspa_txscript_errors::TxScriptError;

/// RuntimeSigOpCounter represents the state tracking of signature operations during script execution.
/// Unlike the static counting approach which counts all possible signature operations,
/// this tracks only the actually executed signature operations, leading to more accurate
/// mass calculations and potentially lower fees for conditional scripts.
#[derive(Debug, Clone)]
pub struct RuntimeSigOpCounter {
    /// Maximum number of signature operations allowed for this input (decoded actual value)
    sig_op_limit: u16,
    /// Remaining signature operations that can be executed
    sig_op_remaining: u16,
}

impl RuntimeSigOpCounter {
    /// Creates a new RuntimeSigOpCounter from a sig_op_limit.
    ///
    /// # Arguments
    /// * `sig_op_limit` - The sig_op_limit from the transaction input
    pub fn new(sig_op_limit: u8) -> Self {
        Self { sig_op_limit: sig_op_limit as u16, sig_op_remaining: sig_op_limit as u16 }
    }

    /// Attempts to consume a single signature operation.
    ///
    /// This method handles:
    /// - Checking if we have enough remaining operations
    /// - Updating the remaining count if successful
    ///
    /// # Returns
    /// * `Ok(())` if the operation was successfully consumed
    /// * `Err(TxScriptError::ExceededSigOpLimit)` if not enough operations remain
    ///
    /// # Example
    /// ```
    /// let mut counter = kaspa_txscript::runtime_resource_meter::RuntimeSigOpCounter::new(1);
    ///
    /// // Consume 1 operation
    /// counter.consume_sig_op().unwrap(); // Ok(())
    /// assert_eq!(counter.sig_op_remaining(), 0);
    /// assert_eq!(counter.used_sig_ops(), 1);
    /// // Try to consume too many
    /// counter.consume_sig_op().unwrap_err(); // Err(ExceededSigOpLimit)
    /// ```
    pub fn consume_sig_op(&mut self) -> Result<(), TxScriptError> {
        self.sig_op_remaining = self.sig_op_remaining.checked_sub(1).ok_or(TxScriptError::ExceededSigOpLimit(self.sig_op_limit))?;
        Ok(())
    }

    pub fn consume_sig_ops(&mut self, count: u16) -> Result<(), TxScriptError> {
        self.sig_op_remaining =
            self.sig_op_remaining.checked_sub(count).ok_or(TxScriptError::ExceededSigOpLimit(self.sig_op_limit))?;
        Ok(())
    }

    pub fn sig_op_remaining(&self) -> u16 {
        self.sig_op_remaining
    }

    pub fn sig_op_limit(&self) -> u16 {
        self.sig_op_limit
    }

    /// Returns the number of signature operations used (as encoded u8 value)
    pub fn used_sig_ops(&self) -> u16 {
        self.sig_op_limit - self.sig_op_remaining
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeScriptUnitMeter {
    used_sig_ops: u16,
    used_script_units: ScriptUnits,
    accounted_pushed_bytes: u64,
    sigop_script_units: ScriptUnits,
    allowed_script_units: ScriptUnits,
    remaining_script_units: ScriptUnits,
}

impl RuntimeScriptUnitMeter {
    pub fn new(sigop_script_units: ScriptUnits, allowed_script_units: ScriptUnits) -> Self {
        Self {
            used_sig_ops: 0,
            used_script_units: ScriptUnits(0),
            accounted_pushed_bytes: 0,
            sigop_script_units,
            allowed_script_units,
            remaining_script_units: allowed_script_units,
        }
    }

    pub fn used_sig_ops(&self) -> u16 {
        self.used_sig_ops
    }

    pub fn used_script_units(&self) -> ScriptUnits {
        self.used_script_units
    }

    pub fn consume_script_units(&mut self, units: ScriptUnits) -> Result<(), TxScriptError> {
        match self.remaining_script_units.checked_sub(units) {
            Some(new_remaining) => {
                self.used_script_units = self.used_script_units.saturating_add(units);
                self.remaining_script_units = new_remaining;
                Ok(())
            }
            None => {
                let overflow = units - self.remaining_script_units;
                let used_units = self.allowed_script_units + overflow;
                Err(TxScriptError::ExceededScriptUnitsLimit { used_units: used_units.0, allowed_units: self.allowed_script_units.0 })
            }
        }
    }

    pub fn consume_sig_op_cost(&mut self, count: u16) -> Result<(), TxScriptError> {
        self.consume_script_units(ScriptUnits((count as u64).saturating_mul(self.sigop_script_units.0)))?;
        self.used_sig_ops = self.used_sig_ops.saturating_add(count);
        Ok(())
    }

    pub fn charge_newly_pushed_bytes(&mut self, total_pushed_bytes: u64) -> Result<(), TxScriptError> {
        let pushed_bytes_delta = total_pushed_bytes.saturating_sub(self.accounted_pushed_bytes);
        self.consume_script_units(pushed_bytes_delta.into())?;
        self.accounted_pushed_bytes = total_pushed_bytes;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum RuntimeResourceMeter {
    Sigops(RuntimeSigOpCounter),
    ScriptUnits(RuntimeScriptUnitMeter),
}

impl RuntimeResourceMeter {
    pub fn new_sigops(sig_op_limit: u8) -> Self {
        Self::Sigops(RuntimeSigOpCounter::new(sig_op_limit))
    }

    pub fn new_script_units(sigop_script_units: ScriptUnits, allowed_script_units: ScriptUnits) -> Self {
        Self::ScriptUnits(RuntimeScriptUnitMeter::new(sigop_script_units, allowed_script_units))
    }

    pub fn used_sig_ops(&self) -> u16 {
        match self {
            Self::Sigops(counter) => counter.used_sig_ops(),
            Self::ScriptUnits(meter) => meter.used_sig_ops(),
        }
    }

    pub fn used_script_units(&self) -> ScriptUnits {
        match self {
            Self::Sigops(_) => ScriptUnits(0),
            Self::ScriptUnits(meter) => meter.used_script_units(),
        }
    }

    pub fn consume_sig_op_cost(&mut self, count: u16) -> Result<(), TxScriptError> {
        match self {
            Self::Sigops(counter) => counter.consume_sig_ops(count),
            Self::ScriptUnits(meter) => meter.consume_sig_op_cost(count),
        }
    }
}
