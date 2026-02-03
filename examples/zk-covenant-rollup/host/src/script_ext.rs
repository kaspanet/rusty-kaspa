use kaspa_txscript::opcodes::codes::{Op2Dup, OpDiv, OpDup, OpMul, OpRot, OpSize, OpSubstr, OpSwap};
use kaspa_txscript::script_builder::ScriptBuilder;

pub trait ScriptBuilderExt {
    /// Splits top-of-stack byte array at midpoint.
    ///
    /// Expects on stack: [..., byte_array]
    /// Leaves on stack:  [..., first_half, second_half]
    fn split_at_mid(&mut self) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder>;
}

impl ScriptBuilderExt for ScriptBuilder {
    fn split_at_mid(&mut self) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
        self.add_op(OpSize)?
            .add_i64(2)?
            .add_op(OpDiv)?
            .add_op(Op2Dup)?
            .add_i64(0)?
            .add_op(OpSwap)?
            .add_op(OpSubstr)?
            .add_op(OpRot)?
            .add_op(OpRot)?
            .add_op(OpDup)?
            .add_i64(2)?
            .add_op(OpMul)?
            .add_op(OpSubstr)?;
        Ok(self)
    }
}
