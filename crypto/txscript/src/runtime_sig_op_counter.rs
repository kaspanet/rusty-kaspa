use kaspa_txscript_errors::TxScriptError;

/// RuntimeSigOpCounter represents the state tracking of signature operations during script execution.
/// Unlike the static counting approach which counts all possible signature operations,
/// this tracks only the actually executed signature operations, leading to more accurate
/// mass calculations and potentially lower fees for conditional scripts.
#[derive(Debug, Clone)]
pub struct RuntimeSigOpCounter {
    /// Maximum number of signature operations allowed for this input
    sig_op_limit: u8,
    /// Remaining signature operations that can be executed
    sig_op_remaining: u8,
}

impl RuntimeSigOpCounter {
    pub fn new(sig_op_limit: u8) -> Self {
        Self { sig_op_limit, sig_op_remaining: sig_op_limit }
    }
    /// Attempts to consume the specified number of signature operations.
    ///
    /// This method handles:
    /// - Checking if we have enough remaining operations
    /// - Updating the remaining count if successful
    ///
    /// # Returns
    /// * `Ok(())` if the operations were successfully consumed
    /// * `Err(TxScriptError::ExceededSigOpLimit)` if not enough operations remain
    ///
    /// # Example
    /// ```
    /// let mut counter = kaspa_txscript::runtime_sig_op_counter::RuntimeSigOpCounter::new(1);
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

    pub fn sig_op_remaining(&self) -> u8 {
        self.sig_op_remaining
    }
    pub fn sig_op_limit(&self) -> u8 {
        self.sig_op_limit
    }
    pub fn used_sig_ops(&self) -> u8 {
        self.sig_op_limit - self.sig_op_remaining
    }
}

pub trait SigOpConsumer {
    fn consume_sig_op(&mut self) -> Result<(), TxScriptError>;
}

impl SigOpConsumer for RuntimeSigOpCounter {
    fn consume_sig_op(&mut self) -> Result<(), TxScriptError> {
        RuntimeSigOpCounter::consume_sig_op(self)
    }
}
impl SigOpConsumer for Option<RuntimeSigOpCounter> {
    fn consume_sig_op(&mut self) -> Result<(), TxScriptError> {
        if let Some(consumer) = self {
            consumer.consume_sig_op()
        } else {
            Ok(())
        }
    }
}
