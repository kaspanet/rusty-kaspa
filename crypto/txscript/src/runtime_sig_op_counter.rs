use kaspa_consensus_core::mass::decode_sig_op_count;
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
    /// Creates a new RuntimeSigOpCounter from an encoded sig_op_limit.
    ///
    /// # Arguments
    /// * `encoded_sig_op_limit` - The compressed u8 sig_op_limit from the transaction input
    /// * `tx_version` - The transaction version used for decoding
    pub fn new(encoded_sig_op_limit: u8, tx_version: u16) -> Self {
        let decoded_limit = decode_sig_op_count(encoded_sig_op_limit, tx_version);
        Self { sig_op_limit: decoded_limit, sig_op_remaining: decoded_limit }
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
    /// let mut counter = kaspa_txscript::runtime_sig_op_counter::RuntimeSigOpCounter::new(1, 1);
    ///
    /// // Consume 1 operation
    /// counter.consume_sig_op().unwrap(); // Ok(())
    /// assert_eq!(counter.sig_op_remaining(), 0);
    /// assert_eq!(counter.used_sig_ops(), 1);
    /// // Try to consume too many
    /// counter.consume_sig_op().unwrap_err(); // Err(ExceededSigOpLimit)
    /// ```
    pub fn consume_sig_op(&mut self) -> Result<(), TxScriptError> {
        self.sig_op_remaining =
            self.sig_op_remaining.checked_sub(1).ok_or(TxScriptError::ExceededSigOpLimit(self.sig_op_limit as u8))?;
        Ok(())
    }

    pub fn consume_sig_ops(&mut self, count: u16) -> Result<(), TxScriptError> {
        self.sig_op_remaining =
            self.sig_op_remaining.checked_sub(count).ok_or(TxScriptError::ExceededSigOpLimit(self.sig_op_limit as u8))?;
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
