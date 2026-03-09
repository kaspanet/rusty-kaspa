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
        // todo it may use split opcode natively

        self.add_op(OpSize)? // [[1,2,3,4,5,6], 6 ]
            .add_i64(2)? // [[1,2,3,4,5,6], 6, 2 ]
            .add_op(OpDiv)? // [[1,2,3,4,5,6], 3 ]
            .add_op(Op2Dup)? // [[1,2,3,4,5,6], 3, [1,2,3,4,5,6], 3 ]
            .add_i64(0)? // [[1,2,3,4,5,6], 3, [1,2,3,4,5,6], 3, 0 ]
            .add_op(OpSwap)?
            .add_op(OpSubstr)? // [[1,2,3,4,5,6], 3, [1,2,3]]
            .add_op(OpRot)? // [3, [1,2,3], [1,2,3,4,5,6]]
            .add_op(OpRot)? // [[1,2,3], [1,2,3,4,5,6], 3]
            .add_op(OpDup)? // [[1,2,3], [1,2,3,4,5,6], 3, 3]
            .add_i64(2)? // [[1,2,3], [1,2,3,4,5,6], 3, 3, 2]
            .add_op(OpMul)? // [[1,2,3], [1,2,3,4,5,6], 6, 3]
            .add_op(OpSubstr)?; // [[1,2,3], [4,5,6]]

        Ok(self)
    }
}
