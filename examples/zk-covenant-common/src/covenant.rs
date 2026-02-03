use kaspa_consensus_core::constants::TX_VERSION;
use kaspa_txscript::opcodes::codes::{
    OpBlake2b, OpCat, OpCovOutCount, OpData32, OpEqual, OpEqualVerify, OpInputCovenantId, OpSwap, OpTxInputIndex, OpTxOutputSpk,
};
use kaspa_txscript::script_builder::ScriptBuilder;

/// Shared covenant methods used by both inline and rollup covenants.
pub trait CovenantBase {
    type Error;

    /// Hashes the new redeem script and builds the expected SPK bytes.
    ///
    /// SPK = version(2) || OpBlake2b || OpData32 || hash || OpEqual
    ///
    /// Expects on stack: [..., new_redeem_script]
    /// Leaves on stack:  [..., constructed_spk]
    fn hash_redeem_to_spk(&mut self) -> Result<&mut Self, Self::Error>;

    /// Verifies constructed SPK matches the actual output SPK at index 0.
    ///
    /// Expects on stack: [..., constructed_spk]
    /// Leaves on stack:  [...]
    fn verify_output_spk(&mut self) -> Result<&mut Self, Self::Error>;

    /// Verifies current input index is 0.
    ///
    /// Expects on stack: [...]
    /// Leaves on stack:  [...]
    fn verify_input_index_zero(&mut self) -> Result<&mut Self, Self::Error>;

    /// Verifies the covenant has exactly one output.
    ///
    /// Expects on stack: [...]
    /// Leaves on stack:  [...]
    fn verify_covenant_single_output(&mut self) -> Result<&mut Self, Self::Error>;
}

impl CovenantBase for ScriptBuilder {
    type Error = kaspa_txscript::script_builder::ScriptBuilderError;

    fn hash_redeem_to_spk(&mut self) -> Result<&mut Self, Self::Error> {
        self.add_op(OpBlake2b)?;
        let mut data = [0u8; 4];
        data[0..2].copy_from_slice(&TX_VERSION.to_le_bytes());
        data[2] = OpBlake2b;
        data[3] = OpData32;
        self.add_data(&data)?;
        self.add_op(OpSwap)?;
        self.add_op(OpCat)?;
        self.add_data(&[OpEqual])?;
        self.add_op(OpCat)
    }

    fn verify_output_spk(&mut self) -> Result<&mut Self, Self::Error> {
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpTxOutputSpk)?;
        self.add_op(OpEqualVerify)
    }

    fn verify_input_index_zero(&mut self) -> Result<&mut Self, Self::Error> {
        self.add_op(OpTxInputIndex)?;
        self.add_i64(0)?;
        self.add_op(OpEqualVerify)
    }

    fn verify_covenant_single_output(&mut self) -> Result<&mut Self, Self::Error> {
        self.add_op(OpTxInputIndex)?.add_op(OpInputCovenantId)?.add_op(OpCovOutCount)?.add_i64(1)?.add_op(OpEqualVerify)
    }
}
