use kaspa_txscript_errors::TxScriptError;

/// Decodes a compressed signature operation count.
/// 
/// The encoding scheme:
/// - Values 0-100: Direct mapping (no compression)
/// - Values 101-255: Each value represents increments of 10
///   - Formula: actual_sigops = 100 + (encoded - 100) * 10
///   - Example: 104 → 140, 164 → 740, 255 → 1650
///
/// # Arguments
/// * `encoded` - The compressed u8 value
///
/// # Returns
/// The actual (decoded) signature operation count as u16
#[inline]
pub fn decode_sig_op_count(encoded: u8) -> u16 {
    if encoded <= 100 {
        encoded as u16
    } else {
        100 + ((encoded as u16 - 100) * 10)
    }
}

/// Encodes an actual signature operation count into compressed u8 format.
/// 
/// The encoding scheme:
/// - Values 0-100: Direct mapping (no compression)
/// - Values 101-1650: Compressed in increments of 10
///   - Formula: encoded = 100 + (actual_sigops - 100) / 10
///   - Note: Values not divisible by 10 are rounded down
/// - Values >1650: Capped at 255 (representing 1650)
///
/// # Arguments
/// * `actual_sigops` - The actual signature operation count
///
/// # Returns
/// The compressed u8 value (0-255)
#[inline]
pub fn encode_sig_op_count(actual_sigops: u64) -> u8 {
    if actual_sigops <= 100 {
        actual_sigops as u8
    } else {
        let encoded = 100 + ((actual_sigops - 100) / 10);
        encoded.min(255) as u8
    }
}

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
    pub fn new(encoded_sig_op_limit: u8) -> Self {
        let decoded_limit = decode_sig_op_count(encoded_sig_op_limit);
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
        self.sig_op_remaining = self.sig_op_remaining.checked_sub(1).ok_or(TxScriptError::ExceededSigOpLimit(self.sig_op_limit as u8))?;
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
    pub fn used_sig_ops(&self) -> u8 {
        encode_sig_op_count((self.sig_op_limit - self.sig_op_remaining) as u64)
    }

    /// Returns the actual number of signature operations used (decoded value)
    pub fn used_sig_ops_actual(&self) -> u16 {
        self.sig_op_limit - self.sig_op_remaining
    }
}

pub trait SigOpConsumer {
    fn consume_sig_op(&mut self) -> Result<(), TxScriptError>;
    fn consume_sig_ops(&mut self, count: u16) -> Result<(), TxScriptError>;
}

impl SigOpConsumer for RuntimeSigOpCounter {
    fn consume_sig_op(&mut self) -> Result<(), TxScriptError> {
        RuntimeSigOpCounter::consume_sig_op(self)
    }
    fn consume_sig_ops(&mut self, count: u16) -> Result<(), TxScriptError> {
        RuntimeSigOpCounter::consume_sig_ops(self, count)
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
    fn consume_sig_ops(&mut self, count: u16) -> Result<(), TxScriptError> {
        if let Some(consumer) = self {
            consumer.consume_sig_ops(count)
        } else {
            Ok(())
        }
    }
}