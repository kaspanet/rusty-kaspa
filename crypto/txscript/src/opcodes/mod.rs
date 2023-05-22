use core::mem::size_of;

#[macro_use]
mod macros;

use crate::data_stack::{DataStack, OpcodeData};
use crate::{
    ScriptSource, TxScriptEngine, TxScriptError, LOCK_TIME_THRESHOLD, MAX_TX_IN_SEQUENCE_NUM, NO_COST_OPCODE,
    SEQUENCE_LOCK_TIME_DISABLED, SEQUENCE_LOCK_TIME_MASK,
};
use blake2b_simd::Params;
use core::cmp::{max, min};
use kaspa_consensus_core::hashing::sighash_type::SigHashType;
use kaspa_consensus_core::tx::VerifiableTransaction;
use sha2::{Digest, Sha256};
use std::fmt::{Debug, Formatter};

/// First value in the range formed by the "small integer" Op# opcodes
pub const OP_SMALL_INT_MIN_VAL: u8 = 1;
/// Last value in the range formed by the "small integer" Op# opcodes
pub const OP_SMALL_INT_MAX_VAL: u8 = 16;
/// First value in the range formed by OpData# opcodes (where opcode == value)
pub const OP_DATA_MIN_VAL: u8 = self::codes::OpData1;
/// Last value in the range formed by OpData# opcodes (where opcode == value)
pub const OP_DATA_MAX_VAL: u8 = self::codes::OpData75;
/// Minus 1 value
pub const OP_1_NEGATE_VAL: u8 = 0x81;

#[derive(PartialEq, Eq)]
pub(crate) enum OpCond {
    False,
    True,
    Skip,
}

impl OpCond {
    pub fn negate(&self) -> OpCond {
        match self {
            OpCond::True => OpCond::False,
            OpCond::False => OpCond::True,
            OpCond::Skip => OpCond::Skip,
        }
    }
}

type OpCodeResult = Result<(), TxScriptError>;

pub(crate) struct OpCode<const CODE: u8> {
    data: Vec<u8>,
}

impl<const CODE: u8> Debug for OpCode<CODE> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Opcode<{:#2x}>{{ data:{:?} }}", CODE, self.data)
    }
}

pub trait OpCodeMetadata: Debug {
    // Opcode number
    fn value(&self) -> u8;
    // length of data
    fn len(&self) -> usize;
    // Conditional should be executed also is not in branch
    fn is_conditional(&self) -> bool;
    // For push data- check if we can use shorter encoding
    fn check_minimal_data_push(&self) -> Result<(), TxScriptError>;

    fn is_push_opcode(&self) -> bool;
    fn get_data(&self) -> &[u8];

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub trait OpCodeExecution<T: VerifiableTransaction> {
    fn empty() -> Result<Box<dyn OpCodeImplementation<T>>, TxScriptError>
    where
        Self: Sized;
    #[allow(clippy::new_ret_no_self)]
    fn new(data: Vec<u8>) -> Result<Box<dyn OpCodeImplementation<T>>, TxScriptError>
    where
        Self: Sized;

    fn execute(&self, vm: &mut TxScriptEngine<T>) -> OpCodeResult;
}

pub trait OpcodeSerialization {
    fn serialize(&self) -> Vec<u8>;
    fn deserialize<'i, I: Iterator<Item = &'i u8>, T: VerifiableTransaction>(
        it: &mut I,
    ) -> Result<Box<dyn OpCodeImplementation<T>>, TxScriptError>
    where
        Self: Sized;
}

pub trait OpCodeImplementation<T: VerifiableTransaction>: OpCodeExecution<T> + OpCodeMetadata + OpcodeSerialization {}

impl<const CODE: u8> OpCodeMetadata for OpCode<CODE> {
    fn value(&self) -> u8 {
        CODE
    }

    fn is_push_opcode(&self) -> bool {
        CODE <= NO_COST_OPCODE
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    // TODO: add it to opcode specification
    fn is_conditional(&self) -> bool {
        self.value() >= 0x63 && self.value() >= 0x68
    }

    fn check_minimal_data_push(&self) -> Result<(), TxScriptError> {
        let data_len = self.len();
        let opcode = self.value();

        if data_len == 0 {
            if opcode != codes::OpFalse {
                return Err(TxScriptError::NotMinimalData(format!(
                    "zero length data push is encoded with opcode {self:?} instead of OpFalse"
                )));
            }
        } else if data_len == 1 && OP_SMALL_INT_MIN_VAL <= self.data[0] && self.data[0] <= OP_SMALL_INT_MAX_VAL {
            if opcode != codes::OpTrue + self.data[0] - 1 {
                return Err(TxScriptError::NotMinimalData(format!(
                    "zero length data push is encoded with opcode {:?} instead of Op_{}",
                    self, self.data[0]
                )));
            }
        } else if data_len == 1 && self.data[0] == OP_1_NEGATE_VAL {
            if opcode != codes::Op1Negate {
                return Err(TxScriptError::NotMinimalData(format!(
                    "data push of the value -1 encoded \
                                    with opcode {self:?} instead of OP_1NEGATE"
                )));
            }
        } else if data_len <= OP_DATA_MAX_VAL as usize {
            if opcode as usize != data_len {
                return Err(TxScriptError::NotMinimalData(format!(
                    "data push of {data_len} bytes encoded \
                                    with opcode {self:?} instead of OP_DATA_{data_len}"
                )));
            }
        } else if data_len <= u8::MAX as usize {
            if opcode != codes::OpPushData1 {
                return Err(TxScriptError::NotMinimalData(format!(
                    "data push of {data_len} bytes encoded \
                                    with opcode {self:?} instead of OP_PUSHDATA1"
                )));
            }
        } else if data_len < u16::MAX as usize && opcode != codes::OpPushData2 {
            return Err(TxScriptError::NotMinimalData(format!(
                "data push of {data_len} bytes encoded \
                                with opcode {self:?} instead of OP_PUSHDATA2"
            )));
        }
        Ok(())
    }

    fn get_data(&self) -> &[u8] {
        &self.data
    }
}

// Helpers for some opcodes with shared data
#[inline]
fn push_data<T: VerifiableTransaction>(data: Vec<u8>, vm: &mut TxScriptEngine<T>) -> OpCodeResult {
    vm.dstack.push(data);
    Ok(())
}

#[inline]
fn push_number<T: VerifiableTransaction>(number: i64, vm: &mut TxScriptEngine<T>) -> OpCodeResult {
    vm.dstack.push_item(number);
    Ok(())
}

/*
The following is the implementation and metadata of all opcodes. Each opcode has unique
number (and template system makes it impossible to use two opcodes), length specification,
and execution code.

The syntax is as follows:
```
opcode OpCodeName<id, length>(self, vm) {
    code;
    output
}
// OR
opcode OpCodeName<id, length>(self, vm) statement

// in case of an opcode alias
opcode |OpCodeAlias| OpCodeName<id, length>(self, vm) {
    code;
    output
}
// OR
opcode |OpCodeAlias| OpCodeName<id, length>(self, vm) statement
```

Length specification is either a number (for fixed length) or a unsigned integer type
(for var length).
The execution code is implementing OpCodeImplementation. You can access the engine using the `vm`
variable.

Implementation details in `opcodes/macros.rs`.
*/
opcode_list! {

    // Data push opcodes.
    opcode |Op0| OpFalse<0x00, 1>(self , vm) {
        vm.dstack.push(vec![]);
        Ok(())
    }

    opcode OpData1<0x01, 2>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData2<0x02, 3>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData3<0x03, 4>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData4<0x04, 5>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData5<0x05, 6>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData6<0x06, 7>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData7<0x07, 8>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData8<0x08, 9>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData9<0x09, 10>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData10<0x0a, 11>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData11<0x0b, 12>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData12<0x0c, 13>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData13<0x0d, 14>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData14<0x0e, 15>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData15<0x0f, 16>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData16<0x10, 17>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData17<0x11, 18>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData18<0x12, 19>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData19<0x13, 20>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData20<0x14, 21>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData21<0x15, 22>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData22<0x16, 23>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData23<0x17, 24>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData24<0x18, 25>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData25<0x19, 26>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData26<0x1a, 27>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData27<0x1b, 28>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData28<0x1c, 29>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData29<0x1d, 30>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData30<0x1e, 31>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData31<0x1f, 32>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData32<0x20, 33>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData33<0x21, 34>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData34<0x22, 35>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData35<0x23, 36>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData36<0x24, 37>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData37<0x25, 38>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData38<0x26, 39>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData39<0x27, 40>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData40<0x28, 41>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData41<0x29, 42>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData42<0x2a, 43>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData43<0x2b, 44>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData44<0x2c, 45>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData45<0x2d, 46>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData46<0x2e, 47>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData47<0x2f, 48>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData48<0x30, 49>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData49<0x31, 50>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData50<0x32, 51>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData51<0x33, 52>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData52<0x34, 53>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData53<0x35, 54>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData54<0x36, 55>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData55<0x37, 56>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData56<0x38, 57>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData57<0x39, 58>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData58<0x3a, 59>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData59<0x3b, 60>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData60<0x3c, 61>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData61<0x3d, 62>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData62<0x3e, 63>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData63<0x3f, 64>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData64<0x40, 65>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData65<0x41, 66>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData66<0x42, 67>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData67<0x43, 68>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData68<0x44, 69>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData69<0x45, 70>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData70<0x46, 71>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData71<0x47, 72>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData72<0x48, 73>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData73<0x49, 74>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData74<0x4a, 75>(self, vm) push_data(self.data.clone(), vm)
    opcode OpData75<0x4b, 76>(self, vm) push_data(self.data.clone(), vm)
    opcode OpPushData1<0x4c, u8>(self, vm) push_data(self.data.clone(), vm)
    opcode OpPushData2<0x4d, u16>(self, vm) push_data(self.data.clone(), vm)
    opcode OpPushData4<0x4e, u32>(self, vm) push_data(self.data.clone(), vm)

    opcode Op1Negate<0x4f, 1>(self, vm) push_number(-1, vm)

    opcode OpReserved<0x50, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{self:?}")))

    opcode |Op1| OpTrue<0x51, 1>(self, vm) push_number(1, vm)
    opcode Op2<0x52, 1>(self, vm) push_number(2, vm)
    opcode Op3<0x53, 1>(self, vm) push_number(3, vm)
    opcode Op4<0x54, 1>(self, vm) push_number(4, vm)
    opcode Op5<0x55, 1>(self, vm) push_number(5, vm)
    opcode Op6<0x56, 1>(self, vm) push_number(6, vm)
    opcode Op7<0x57, 1>(self, vm) push_number(7, vm)
    opcode Op8<0x58, 1>(self, vm) push_number(8, vm)
    opcode Op9<0x59, 1>(self, vm) push_number(9, vm)
    opcode Op10<0x5a, 1>(self, vm) push_number(10, vm)
    opcode Op11<0x5b, 1>(self, vm) push_number(11, vm)
    opcode Op12<0x5c, 1>(self, vm) push_number(12, vm)
    opcode Op13<0x5d, 1>(self, vm) push_number(13, vm)
    opcode Op14<0x5e, 1>(self, vm) push_number(14, vm)
    opcode Op15<0x5f, 1>(self, vm) push_number(15, vm)
    opcode Op16<0x60, 1>(self, vm) push_number(16, vm)

    // Control opcodes.
    opcode OpNop<0x61, 1>(self, vm) Ok(())
    opcode OpVer<0x62, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{self:?}")))

    opcode OpIf<0x63, 1>(self, vm) {
        let mut cond = OpCond::Skip;
        if vm.is_executing() {
            if let Some(mut cond_buf) = vm.dstack.pop() {
                if cond_buf.len() > 1 {
                    return Err(TxScriptError::InvalidState("expected boolean".to_string()));
                }
                cond = match cond_buf.pop() {
                    Some(stack_cond) => match stack_cond {
                        1 => OpCond::True,
                        _ => return Err(TxScriptError::InvalidState("expected boolean".to_string())),
                    }
                    None => OpCond::False,
                }
            } else {
                return Err(TxScriptError::EmptyStack);
            }
        }
        vm.cond_stack.push(cond);
        Ok(())
    }

    opcode OpNotIf<0x64, 1>(self, vm) {
        let mut cond = OpCond::Skip;
        if vm.is_executing() {
            if let Some(mut cond_buf) = vm.dstack.pop() {
                if cond_buf.len() > 1 {
                    return Err(TxScriptError::InvalidState("expected boolean".to_string()));
                }
                cond = match cond_buf.pop() {
                    Some(stack_cond) => match stack_cond {
                        1 => OpCond::False,
                        _ => return Err(TxScriptError::InvalidState("expected boolean".to_string())),
                    }
                    None => OpCond::True,
                }
            } else {
                return Err(TxScriptError::EmptyStack);
            }
        }
        vm.cond_stack.push(cond);
        Ok(())
    }

    opcode OpVerIf<0x65, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{self:?}")))
    opcode OpVerNotIf<0x66, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{self:?}")))

    opcode OpElse<0x67, 1>(self, vm) {
        if let Some(cond) = vm.cond_stack.last_mut() {
            *cond = cond.negate();
            Ok(())
        } else {
            Err(TxScriptError::InvalidState("condition stack empty".to_string()))
        }
    }

    opcode OpEndIf<0x68, 1>(self, vm) {
        match vm.cond_stack.pop() {
            None => Err(TxScriptError::InvalidState("condition stack empty".to_string())),
            _ => Ok(())
        }
    }

    opcode OpVerify<0x69, 1>(self, vm) {
        let [result]: [bool; 1] = vm.dstack.pop_items()?;
        match result {
            true => Ok(()),
            false => Err(TxScriptError::VerifyError)
        }
    }

    opcode OpReturn<0x6a, 1>(self, vm) Err(TxScriptError::EarlyReturn)

    // Stack opcodes.
    opcode OpToAltStack<0x6b, 1>(self, vm) {
        let [item] = vm.dstack.pop_raw()?;
        vm.astack.push(item);
        Ok(())
    }

    opcode OpFromAltStack<0x6c, 1>(self, vm) {
        match vm.astack.pop() {
            Some(last) => {
                vm.dstack.push(last);
                Ok(())
            },
            None => Err(TxScriptError::EmptyStack)
        }
    }

    opcode Op2Drop<0x6d, 1>(self, vm) vm.dstack.drop_items::<2>()
    opcode Op2Dup<0x6e, 1>(self, vm) vm.dstack.dup_items::<2>()
    opcode Op3Dup<0x6f, 1>(self, vm) vm.dstack.dup_items::<3>()
    opcode Op2Over<0x70, 1>(self, vm) vm.dstack.over_items::<2>()
    opcode Op2Rot<0x71, 1>(self, vm) vm.dstack.rot_items::<2>()
    opcode Op2Swap<0x72, 1>(self, vm) vm.dstack.swap_items::<2>()

    opcode OpIfDup<0x73, 1>(self, vm) {
        let [result] = vm.dstack.pop_raw()?;
        if <Vec<u8> as OpcodeData<bool>>::deserialize(&result)? {
            vm.dstack.push(result);
        }
        Ok(())
    }

    opcode OpDepth<0x74, 1>(self, vm) push_number(vm.dstack.len() as i64, vm)

    opcode OpDrop<0x75, 1>(self, vm) vm.dstack.drop_items::<1>()
    opcode OpDup<0x76, 1>(self, vm) vm.dstack.dup_items::<1>()

    opcode OpNip<0x77, 1>(self, vm) {
        match vm.dstack.len() >= 2 {
            true => {
                vm.dstack.remove(vm.dstack.len()-2);
                Ok(())
            }
            false => Err(TxScriptError::EmptyStack),
        }
    }

    opcode OpOver<0x78, 1>(self, vm) vm.dstack.over_items::<1>()

    opcode OpPick<0x79, 1>(self, vm) {
        let [loc]: [i32; 1] = vm.dstack.pop_items()?;
        match loc >= 0 {
            true => {
                vm.dstack.push(vm.dstack[vm.dstack.len()-(loc as usize)-1].clone());
                Ok(())
            }
            false => Err(TxScriptError::InvalidState("pick at a negative value".to_string()))
        }
    }

    opcode OpRoll<0x7a, 1>(self, vm) {
        let [loc]: [i32; 1] = vm.dstack.pop_items()?;
        match loc >= 0 {
            true => {
                let item = vm.dstack.remove(vm.dstack.len()-(loc as usize)-1);
                vm.dstack.push(item);
                Ok(())
            }
            false => Err(TxScriptError::InvalidState("pick at a negative value".to_string()))
        }
    }

    opcode OpRot<0x7b, 1>(self, vm) vm.dstack.rot_items::<1>()
    opcode OpSwap<0x7c, 1>(self, vm) vm.dstack.swap_items::<1>()

    opcode OpTuck<0x7d, 1>(self, vm) {
        match vm.dstack.len() >= 2 {
            true => {
                vm.dstack.insert(vm.dstack.len()-2, vm.dstack.last().expect("We have at least two items").clone());
                Ok(())
            }
            false => Err(TxScriptError::EmptyStack)
        }
    }

    // Splice opcodes.
    opcode OpCat<0x7e, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))
    opcode OpSubStr<0x7f, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))
    opcode OpLeft<0x80, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))
    opcode OpRight<0x81, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))

    opcode OpSize<0x82, 1>(self, vm) {
        match vm.dstack.last() {
            Some(last) => {
                vm.dstack.push_item(i64::try_from(last.len()).map_err(|e| TxScriptError::NumberTooBig(e.to_string()))?);
                Ok(())
            },
            None => Err(TxScriptError::EmptyStack)
        }
    }

    // Bitwise logic opcodes.
    opcode OpInvert<0x83, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))
    opcode OpAnd<0x84, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))
    opcode OpOr<0x85, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))
    opcode OpXor<0x86, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))

    opcode OpEqual<0x87, 1>(self, vm) {
        match vm.dstack.len() >= 2 {
            true => {
                let pair = vm.dstack.split_off(vm.dstack.len() - 2);
                match pair[0] == pair[1] {
                    true => vm.dstack.push(vec![1]),
                    false => vm.dstack.push(vec![]),
                }
                Ok(())
            }
            false => Err(TxScriptError::EmptyStack)
        }
    }

    opcode OpEqualVerify<0x88, 1>(self, vm) {
        match vm.dstack.len() >= 2 {
            true => {
                let pair = vm.dstack.split_off(vm.dstack.len() - 2);
                match pair[0] == pair[1] {
                    true => Ok(()),
                    false => Err(TxScriptError::VerifyError),
                }
            }
            false => Err(TxScriptError::EmptyStack)
        }
    }

    opcode OpReserved1<0x89, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{self:?}")))
    opcode OpReserved2<0x8a, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{self:?}")))

    // Numeric related opcodes.
    opcode Op1Add<0x8b, 1>(self, vm) {
        let [value]: [i64; 1] = vm.dstack.pop_items()?;
        vm.dstack.push_item(value + 1);
        Ok(())
    }

    opcode Op1Sub<0x8c, 1>(self, vm) {
        let [value]: [i64; 1] = vm.dstack.pop_items()?;
        vm.dstack.push_item(value - 1);
        Ok(())
    }

    opcode Op2Mul<0x8d, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))
    opcode Op2Div<0x8e, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))

    opcode OpNegate<0x8f, 1>(self, vm) {
        let [value]: [i64; 1] = vm.dstack.pop_items()?;
        vm.dstack.push_item(-value);
        Ok(())
    }

    opcode OpAbs<0x90, 1>(self, vm) {
        let [m]: [i64; 1] = vm.dstack.pop_items()?;
        vm.dstack.push_item(m.abs());
        Ok(())
    }

    opcode OpNot<0x91, 1>(self, vm) {
        let [m]: [i64; 1] = vm.dstack.pop_items()?;
        vm.dstack.push_item((m == 0) as i64);
        Ok(())
    }

    opcode Op0NotEqual<0x92, 1>(self, vm) {
        let [m]: [i64; 1] = vm.dstack.pop_items()?;
        vm.dstack.push_item((m != 0) as i64 );
        Ok(())
    }

    opcode OpAdd<0x93, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item(a+b);
        Ok(())
    }

    opcode OpSub<0x94, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item(a-b);
        Ok(())
    }

    opcode OpMul<0x95, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))
    opcode OpDiv<0x96, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))
    opcode OpMod<0x97, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))
    opcode OpLShift<0x98, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))
    opcode OpRShift<0x99, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{self:?}")))

    opcode OpBoolAnd<0x9a, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item(((a != 0) && (b != 0)) as i64);
        Ok(())
    }

    opcode OpBoolOr<0x9b, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item(((a != 0) || (b != 0)) as i64);
        Ok(())
    }

    opcode OpNumEqual<0x9c, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item((a == b) as i64);
        Ok(())
    }

    opcode OpNumEqualVerify<0x9d, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        match a == b {
            true => Ok(()),
            false => Err(TxScriptError::VerifyError)
        }
    }

    opcode OpNumNotEqual<0x9e, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item((a != b) as i64);
        Ok(())
    }

    opcode OpLessThan<0x9f, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item((a < b) as i64);
        Ok(())
    }

    opcode OpGreaterThan<0xa0, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item((a > b) as i64);
        Ok(())
    }

    opcode OpLessThanOrEqual<0xa1, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item((a <= b) as i64);
        Ok(())
    }

    opcode OpGreaterThanOrEqual<0xa2, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item((a >= b) as i64);
        Ok(())
    }

    opcode OpMin<0xa3, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item(min(a,b));
        Ok(())
    }

    opcode OpMax<0xa4, 1>(self, vm) {
        let [a,b]: [i64; 2] = vm.dstack.pop_items()?;
        vm.dstack.push_item(max(a,b));
        Ok(())
    }

    opcode OpWithin<0xa5, 1>(self, vm) {
        let [x,l,u]: [i64; 3] = vm.dstack.pop_items()?;
        vm.dstack.push_item((x >= l && x < u) as i64);
        Ok(())
    }

    // Undefined opcodes.
    opcode OpUnknown166<0xa6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown167<0xa7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))

    // Crypto opcodes.
    opcode OpSHA256<0xa8, 1>(self, vm) {
        let [last] = vm.dstack.pop_raw()?;
        let mut hasher = Sha256::new();
        hasher.update(last);
        vm.dstack.push(hasher.finalize().to_vec());
        Ok(())
    }

    opcode OpCheckMultiSigECDSA<0xa9, 1>(self, vm) {
        vm.op_check_multisig_schnorr_or_ecdsa(true)
    }

    opcode OpBlake2b<0xaa, 1>(self, vm) {
        let [last] = vm.dstack.pop_raw()?;
        //let hash = blake2b(last.as_slice());
        let hash = Params::new().hash_length(32).to_state().update(&last).finalize();
        vm.dstack.push(hash.as_bytes().to_vec());
        Ok(())
    }

    opcode OpCheckSigECDSA<0xab, 1>(self, vm) {
        let [mut sig, key] = vm.dstack.pop_raw()?;
        // Hash type
        match sig.pop() {
            Some(typ) => {
                let hash_type = SigHashType::from_u8(typ).map_err(|e| TxScriptError::InvalidSigHashType(typ))?;
                match vm.check_ecdsa_signature(hash_type, key.as_slice(), sig.as_slice()) {
                    Ok(valid) => {
                        vm.dstack.push_item(valid);
                        Ok(())
                    },
                    Err(e) => {
                        Err(e)
                    }
                }
            }
            None => {
                vm.dstack.push_item(false);
                Ok(())
            }
        }
    }

    opcode OpCheckSig<0xac, 1>(self, vm) {
        let [mut sig, key] = vm.dstack.pop_raw()?;
        // Hash type
        match sig.pop() {
            Some(typ) => {
                let hash_type = SigHashType::from_u8(typ).map_err(|e| TxScriptError::InvalidSigHashType(typ))?;
                match vm.check_schnorr_signature(hash_type, key.as_slice(), sig.as_slice()) {
                    Ok(valid) => {
                        vm.dstack.push_item(valid);
                        Ok(())
                    },
                    Err(e) => {
                        Err(e)
                    }
                }
            }
            None => {
                vm.dstack.push_item(false);
                Ok(())
            }
        }
    }

    opcode OpCheckSigVerify<0xad, 1>(self, vm) {
        // TODO: when changing impl to array based, change this too
        OpCheckSig{data: self.data.clone()}.execute(vm)?;
        let [valid]: [bool; 1] = vm.dstack.pop_items()?;
        match valid {
            true => Ok(()),
            false => Err(TxScriptError::VerifyError)
        }
    }

    opcode OpCheckMultiSig<0xae, 1>(self, vm) {
        vm.op_check_multisig_schnorr_or_ecdsa(false)
    }

    opcode OpCheckMultiSigVerify<0xaf, 1>(self, vm) {
        // TODO: when changing impl to array based, change this too
        OpCheckMultiSig{data: self.data.clone()}.execute(vm)?;
        let [valid]: [bool; 1] = vm.dstack.pop_items()?;
        match valid {
            true => Ok(()),
            false => Err(TxScriptError::VerifyError)
        }
    }

    opcode OpCheckLockTimeVerify<0xb0, 1>(self, vm) {
        match vm.script_source {
            ScriptSource::TxInput {input, tx, ..} => {
                let [mut lock_time_bytes] = vm.dstack.pop_raw()?;

                // Make sure lockTimeBytes is exactly 8 bytes.
                // If more - return ErrNumberTooBig
                // If less - pad with 0's
                if lock_time_bytes.len() > 8 {
                    return Err(TxScriptError::NumberTooBig(format!("lockTime value represented as {lock_time_bytes:x?} is longer then 8 bytes")))
                }
                lock_time_bytes.resize(8, 0);
                let stack_lock_time = u64::from_le_bytes(lock_time_bytes.try_into().expect("checked vector size"));

                // The lock time field of a transaction is either a DAA score at
                // which the transaction is finalized or a timestamp depending on if the
                // value is before the constants.LockTimeThreshold. When it is under the
                // threshold it is a DAA score.
                if !(
                    (tx.tx().lock_time < LOCK_TIME_THRESHOLD && stack_lock_time < LOCK_TIME_THRESHOLD) ||
                    (tx.tx().lock_time >= LOCK_TIME_THRESHOLD && stack_lock_time >= LOCK_TIME_THRESHOLD)
                ){
                    return Err(TxScriptError::UnsatisfiedLockTime(format!("mismatched locktime types -- tx locktime {}, stack locktime {}", tx.tx().lock_time, stack_lock_time)))
                }

                if stack_lock_time > tx.tx().lock_time {
                    return Err(TxScriptError::UnsatisfiedLockTime(format!("locktime requirement not satisfied -- locktime is greater than the transaction locktime: {} > {}", stack_lock_time, tx.tx().lock_time)))
                }

                // The lock time feature can also be disabled, thereby bypassing
                // OP_CHECKLOCKTIMEVERIFY, if every transaction input has been finalized by
                // setting its sequence to the maximum value (constants.MaxTxInSequenceNum). This
                // condition would result in the transaction being allowed into the blockDAG
                // making the opcode ineffective.
                //
                // This condition is prevented by enforcing that the input being used by
                // the opcode is unlocked (its sequence number is less than the max
                // value). This is sufficient to prove correctness without having to
                // check every input.
                //
                // NOTE: This implies that even if the transaction is not finalized due to
                // another input being unlocked, the opcode execution will still fail when the
                // input being used by the opcode is locked.
                if input.sequence == MAX_TX_IN_SEQUENCE_NUM {
                    return Err(TxScriptError::UnsatisfiedLockTime("transaction input is finalized".to_string()));
                }
                Ok(())
            }
            _ => Err(TxScriptError::InvalidSource("LockTimeVerify only applies to transaction inputs".to_string()))
        }
    }

    opcode OpCheckSequenceVerify<0xb1, 1>(self, vm) {
        match vm.script_source {
            ScriptSource::TxInput {input, tx, ..} => {
                let [mut sequence_bytes] = vm.dstack.pop_raw()?;

                // Make sure sequenceBytes is exactly 8 bytes.
                // If more - return ErrNumberTooBig
                // If less - pad with 0's
                if sequence_bytes.len() > 8 {
                    return Err(TxScriptError::NumberTooBig(format!("lockTime value represented as {sequence_bytes:x?} is longer then 8 bytes")))
                }
                // Don't use makeScriptNum here, since sequence is not an actual number, minimal encoding rules don't apply to it,
                // and is more convenient to be represented as an unsigned int.
                sequence_bytes.resize(8, 0);
                let stack_sequence = u64::from_le_bytes(sequence_bytes.try_into().expect("ensured size checks"));

                // To provide for future soft-fork extensibility, if the
                // operand has the disabled lock-time flag set,
                // CHECKSEQUENCEVERIFY behaves as a NOP.
                if stack_sequence & SEQUENCE_LOCK_TIME_DISABLED != 0 {
                    return Ok(());
                }

                // Sequence numbers with their most significant bit set are not
                // consensus constrained. Testing that the transaction's sequence
                // number does not have this bit set prevents using this property
                // to get around a CHECKSEQUENCEVERIFY check.
                if input.sequence & SEQUENCE_LOCK_TIME_DISABLED != 0 {
                    return Err(TxScriptError::UnsatisfiedLockTime(format!("transaction sequence has sequence locktime disabled bit set: {:#x}", input.sequence)));
                }

                // Mask off non-consensus bits before doing comparisons.
                if (stack_sequence & SEQUENCE_LOCK_TIME_MASK) > (input.sequence & SEQUENCE_LOCK_TIME_MASK) {
                    return Err(TxScriptError::UnsatisfiedLockTime(format!("locktime requirement not satisfied -- locktime is greater than the transaction locktime: {} > {}", stack_sequence & SEQUENCE_LOCK_TIME_MASK, input.sequence & SEQUENCE_LOCK_TIME_MASK)))
                }
                Ok(())
            }
            _ => Err(TxScriptError::InvalidSource("LockTimeVerify only applies to transaction inputs".to_string()))
        }
    }

    // Undefined opcodes.
    opcode OpUnknown178<0xb2, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown179<0xb3, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown180<0xb4, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown181<0xb5, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown182<0xb6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown183<0xb7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown184<0xb8, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown185<0xb9, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown186<0xba, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown187<0xbb, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown188<0xbc, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown189<0xbd, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown190<0xbe, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown191<0xbf, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown192<0xc0, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown193<0xc1, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown194<0xc2, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown195<0xc3, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown196<0xc4, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown197<0xc5, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown198<0xc6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown199<0xc7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown200<0xc8, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown201<0xc9, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown202<0xca, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown203<0xcb, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown204<0xcc, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown205<0xcd, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown206<0xce, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown207<0xcf, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown208<0xd0, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown209<0xd1, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown210<0xd2, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown211<0xd3, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown212<0xd4, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown213<0xd5, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown214<0xd6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown215<0xd7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown216<0xd8, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown217<0xd9, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown218<0xda, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown219<0xdb, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown220<0xdc, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown221<0xdd, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown222<0xde, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown223<0xdf, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown224<0xe0, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown225<0xe1, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown226<0xe2, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown227<0xe3, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown228<0xe4, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown229<0xe5, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown230<0xe6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown231<0xe7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown232<0xe8, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown233<0xe9, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown234<0xea, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown235<0xeb, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown236<0xec, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown237<0xed, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown238<0xee, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown239<0xef, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown240<0xf0, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown241<0xf1, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown242<0xf2, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown243<0xf3, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown244<0xf4, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown245<0xf5, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown246<0xf6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown247<0xf7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown248<0xf8, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown249<0xf9, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))

    opcode OpSmallInteger<0xfa, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpPubKeys<0xfb, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpUnknown252<0xfc, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpPubKeyHash<0xfd, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpPubKey<0xfe, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
    opcode OpInvalidOpCode<0xff, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{self:?}")))
}

// converts an opcode from the list of Op0 to Op16 to its associated value
#[allow(clippy::borrowed_box)]
pub fn to_small_int<T: VerifiableTransaction>(opcode: &Box<dyn OpCodeImplementation<T>>) -> u8 {
    let value = opcode.value();
    if value == codes::OpFalse {
        return 0;
    }

    assert!((codes::OpTrue..codes::Op16).contains(&value), "expected op codes between from the list of Op0 to Op16");
    value - (codes::OpTrue - 1)
}

#[cfg(test)]
mod test {
    use crate::caches::Cache;
    use crate::data_stack::Stack;
    use crate::opcodes::{OpCodeExecution, OpCodeImplementation};
    use crate::{opcodes, TxScriptEngine, TxScriptError};
    use kaspa_consensus_core::hashing::sighash::SigHashReusedValues;
    use kaspa_consensus_core::tx::PopulatedTransaction;

    struct TestCase<'a> {
        init: Stack,
        code: Box<dyn OpCodeImplementation<PopulatedTransaction<'a>>>,
        dstack: Stack,
    }

    fn run_test_cases(tests: Vec<TestCase>) {
        let cache = Cache::new(10_000);
        let mut reused_values = SigHashReusedValues::new();
        for TestCase { init, code, dstack } in tests {
            let mut vm = TxScriptEngine::new(&mut reused_values, &cache);
            vm.dstack = init;
            code.execute(&mut vm).expect("Script should not fail");
            assert_eq!(*vm.dstack, dstack, "OpCode {} Pushed wrong value", code.value());
        }
    }

    #[test]
    fn test_opcode_disabled() {
        let tests: Vec<Box<dyn OpCodeImplementation<PopulatedTransaction>>> = vec![
            opcodes::OpCat::empty().expect("Should accept empty"),
            opcodes::OpSubStr::empty().expect("Should accept empty"),
            opcodes::OpLeft::empty().expect("Should accept empty"),
            opcodes::OpRight::empty().expect("Should accept empty"),
            opcodes::OpInvert::empty().expect("Should accept empty"),
            opcodes::OpAnd::empty().expect("Should accept empty"),
            opcodes::OpOr::empty().expect("Should accept empty"),
            opcodes::Op2Mul::empty().expect("Should accept empty"),
            opcodes::Op2Div::empty().expect("Should accept empty"),
            opcodes::OpMul::empty().expect("Should accept empty"),
            opcodes::OpDiv::empty().expect("Should accept empty"),
            opcodes::OpMod::empty().expect("Should accept empty"),
            opcodes::OpLShift::empty().expect("Should accept empty"),
            opcodes::OpRShift::empty().expect("Should accept empty"),
        ];

        let cache = Cache::new(10_000);
        let mut reused_values = SigHashReusedValues::new();
        let mut vm = TxScriptEngine::new(&mut reused_values, &cache);

        for pop in tests {
            match pop.execute(&mut vm) {
                Err(TxScriptError::OpcodeDisabled(_)) => {}
                _ => panic!("Opcode {pop:?} should be disabled"),
            }
        }
    }

    #[test]
    fn test_opcode_reserved() {
        let tests: Vec<Box<dyn OpCodeImplementation<PopulatedTransaction>>> = vec![
            opcodes::OpReserved::empty().expect("Should accept empty"),
            opcodes::OpVer::empty().expect("Should accept empty"),
            opcodes::OpVerIf::empty().expect("Should accept empty"),
            opcodes::OpVerNotIf::empty().expect("Should accept empty"),
            opcodes::OpReserved1::empty().expect("Should accept empty"),
            opcodes::OpReserved2::empty().expect("Should accept empty"),
        ];

        let cache = Cache::new(10_000);
        let mut reused_values = SigHashReusedValues::new();
        let mut vm = TxScriptEngine::new(&mut reused_values, &cache);

        for pop in tests {
            match pop.execute(&mut vm) {
                Err(TxScriptError::OpcodeReserved(_)) => {}
                _ => panic!("Opcode {pop:?} should be disabled"),
            }
        }
    }

    #[test]
    fn test_opcode_invalid() {
        let tests: Vec<Box<dyn OpCodeImplementation<PopulatedTransaction>>> = vec![
            opcodes::OpUnknown166::empty().expect("Should accept empty"),
            opcodes::OpUnknown167::empty().expect("Should accept empty"),
            opcodes::OpUnknown178::empty().expect("Should accept empty"),
            opcodes::OpUnknown179::empty().expect("Should accept empty"),
            opcodes::OpUnknown180::empty().expect("Should accept empty"),
            opcodes::OpUnknown181::empty().expect("Should accept empty"),
            opcodes::OpUnknown182::empty().expect("Should accept empty"),
            opcodes::OpUnknown183::empty().expect("Should accept empty"),
            opcodes::OpUnknown184::empty().expect("Should accept empty"),
            opcodes::OpUnknown185::empty().expect("Should accept empty"),
            opcodes::OpUnknown186::empty().expect("Should accept empty"),
            opcodes::OpUnknown187::empty().expect("Should accept empty"),
            opcodes::OpUnknown188::empty().expect("Should accept empty"),
            opcodes::OpUnknown189::empty().expect("Should accept empty"),
            opcodes::OpUnknown190::empty().expect("Should accept empty"),
            opcodes::OpUnknown191::empty().expect("Should accept empty"),
            opcodes::OpUnknown192::empty().expect("Should accept empty"),
            opcodes::OpUnknown193::empty().expect("Should accept empty"),
            opcodes::OpUnknown194::empty().expect("Should accept empty"),
            opcodes::OpUnknown195::empty().expect("Should accept empty"),
            opcodes::OpUnknown196::empty().expect("Should accept empty"),
            opcodes::OpUnknown197::empty().expect("Should accept empty"),
            opcodes::OpUnknown198::empty().expect("Should accept empty"),
            opcodes::OpUnknown199::empty().expect("Should accept empty"),
            opcodes::OpUnknown200::empty().expect("Should accept empty"),
            opcodes::OpUnknown201::empty().expect("Should accept empty"),
            opcodes::OpUnknown202::empty().expect("Should accept empty"),
            opcodes::OpUnknown203::empty().expect("Should accept empty"),
            opcodes::OpUnknown204::empty().expect("Should accept empty"),
            opcodes::OpUnknown205::empty().expect("Should accept empty"),
            opcodes::OpUnknown206::empty().expect("Should accept empty"),
            opcodes::OpUnknown207::empty().expect("Should accept empty"),
            opcodes::OpUnknown208::empty().expect("Should accept empty"),
            opcodes::OpUnknown209::empty().expect("Should accept empty"),
            opcodes::OpUnknown210::empty().expect("Should accept empty"),
            opcodes::OpUnknown211::empty().expect("Should accept empty"),
            opcodes::OpUnknown212::empty().expect("Should accept empty"),
            opcodes::OpUnknown213::empty().expect("Should accept empty"),
            opcodes::OpUnknown214::empty().expect("Should accept empty"),
            opcodes::OpUnknown215::empty().expect("Should accept empty"),
            opcodes::OpUnknown216::empty().expect("Should accept empty"),
            opcodes::OpUnknown217::empty().expect("Should accept empty"),
            opcodes::OpUnknown218::empty().expect("Should accept empty"),
            opcodes::OpUnknown219::empty().expect("Should accept empty"),
            opcodes::OpUnknown220::empty().expect("Should accept empty"),
            opcodes::OpUnknown221::empty().expect("Should accept empty"),
            opcodes::OpUnknown222::empty().expect("Should accept empty"),
            opcodes::OpUnknown223::empty().expect("Should accept empty"),
            opcodes::OpUnknown224::empty().expect("Should accept empty"),
            opcodes::OpUnknown225::empty().expect("Should accept empty"),
            opcodes::OpUnknown226::empty().expect("Should accept empty"),
            opcodes::OpUnknown227::empty().expect("Should accept empty"),
            opcodes::OpUnknown228::empty().expect("Should accept empty"),
            opcodes::OpUnknown229::empty().expect("Should accept empty"),
            opcodes::OpUnknown230::empty().expect("Should accept empty"),
            opcodes::OpUnknown231::empty().expect("Should accept empty"),
            opcodes::OpUnknown232::empty().expect("Should accept empty"),
            opcodes::OpUnknown233::empty().expect("Should accept empty"),
            opcodes::OpUnknown234::empty().expect("Should accept empty"),
            opcodes::OpUnknown235::empty().expect("Should accept empty"),
            opcodes::OpUnknown236::empty().expect("Should accept empty"),
            opcodes::OpUnknown237::empty().expect("Should accept empty"),
            opcodes::OpUnknown238::empty().expect("Should accept empty"),
            opcodes::OpUnknown239::empty().expect("Should accept empty"),
            opcodes::OpUnknown240::empty().expect("Should accept empty"),
            opcodes::OpUnknown241::empty().expect("Should accept empty"),
            opcodes::OpUnknown242::empty().expect("Should accept empty"),
            opcodes::OpUnknown243::empty().expect("Should accept empty"),
            opcodes::OpUnknown244::empty().expect("Should accept empty"),
            opcodes::OpUnknown245::empty().expect("Should accept empty"),
            opcodes::OpUnknown246::empty().expect("Should accept empty"),
            opcodes::OpUnknown247::empty().expect("Should accept empty"),
            opcodes::OpUnknown248::empty().expect("Should accept empty"),
            opcodes::OpUnknown249::empty().expect("Should accept empty"),
        ];

        let cache = Cache::new(10_000);
        let mut reused_values = SigHashReusedValues::new();
        let mut vm = TxScriptEngine::new(&mut reused_values, &cache);

        for pop in tests {
            match pop.execute(&mut vm) {
                Err(TxScriptError::InvalidOpcode(_)) => {}
                _ => panic!("Opcode {pop:?} should be disabled"),
            }
        }
    }

    #[test]
    fn test_push_data() {
        run_test_cases(vec![
            TestCase { code: opcodes::OpFalse::empty().expect("Should accept empty"), dstack: vec![vec![]], init: Default::default() },
            TestCase {
                code: opcodes::OpData1::new([1u8; 1].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 1].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData2::new([1u8; 2].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 2].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData3::new([1u8; 3].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 3].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData4::new([1u8; 4].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 4].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData5::new([1u8; 5].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 5].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData6::new([1u8; 6].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 6].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData7::new([1u8; 7].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 7].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData8::new([1u8; 8].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 8].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData9::new([1u8; 9].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 9].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData10::new([1u8; 10].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 10].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData11::new([1u8; 11].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 11].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData12::new([1u8; 12].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 12].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData13::new([1u8; 13].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 13].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData14::new([1u8; 14].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 14].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData15::new([1u8; 15].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 15].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData16::new([1u8; 16].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 16].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData17::new([1u8; 17].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 17].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData18::new([1u8; 18].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 18].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData19::new([1u8; 19].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 19].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData20::new([1u8; 20].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 20].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData21::new([1u8; 21].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 21].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData22::new([1u8; 22].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 22].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData23::new([1u8; 23].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 23].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData24::new([1u8; 24].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 24].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData25::new([1u8; 25].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 25].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData26::new([1u8; 26].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 26].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData27::new([1u8; 27].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 27].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData28::new([1u8; 28].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 28].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData29::new([1u8; 29].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 29].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData30::new([1u8; 30].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 30].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData31::new([1u8; 31].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 31].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData32::new([1u8; 32].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 32].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData33::new([1u8; 33].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 33].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData34::new([1u8; 34].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 34].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData35::new([1u8; 35].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 35].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData36::new([1u8; 36].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 36].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData37::new([1u8; 37].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 37].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData38::new([1u8; 38].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 38].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData39::new([1u8; 39].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 39].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData40::new([1u8; 40].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 40].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData41::new([1u8; 41].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 41].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData42::new([1u8; 42].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 42].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData43::new([1u8; 43].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 43].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData44::new([1u8; 44].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 44].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData45::new([1u8; 45].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 45].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData46::new([1u8; 46].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 46].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData47::new([1u8; 47].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 47].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData48::new([1u8; 48].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 48].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData49::new([1u8; 49].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 49].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData50::new([1u8; 50].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 50].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData51::new([1u8; 51].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 51].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData52::new([1u8; 52].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 52].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData53::new([1u8; 53].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 53].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData54::new([1u8; 54].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 54].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData55::new([1u8; 55].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 55].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData56::new([1u8; 56].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 56].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData57::new([1u8; 57].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 57].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData58::new([1u8; 58].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 58].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData59::new([1u8; 59].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 59].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData60::new([1u8; 60].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 60].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData61::new([1u8; 61].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 61].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData62::new([1u8; 62].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 62].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData63::new([1u8; 63].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 63].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData64::new([1u8; 64].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 64].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData65::new([1u8; 65].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 65].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData66::new([1u8; 66].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 66].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData67::new([1u8; 67].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 67].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData68::new([1u8; 68].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 68].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData69::new([1u8; 69].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 69].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData70::new([1u8; 70].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 70].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData71::new([1u8; 71].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 71].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData72::new([1u8; 72].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 72].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData73::new([1u8; 73].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 73].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData74::new([1u8; 74].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 74].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpData75::new([1u8; 75].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 75].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpPushData1::new([1u8; 76].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 76].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpPushData2::new([1u8; 0x100].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 0x100].to_vec()],
                init: Default::default(),
            },
            TestCase {
                code: opcodes::OpPushData4::new([1u8; 0x10000].to_vec()).expect("Valid opcode"),
                dstack: vec![[1u8; 0x10000].to_vec()],
                init: Default::default(),
            },
        ]);
    }

    #[test]
    fn test_push_num() {
        run_test_cases(vec![
            TestCase { code: opcodes::Op1::empty().expect("Should accept empty"), dstack: vec![vec![1]], init: Default::default() },
            TestCase { code: opcodes::Op2::empty().expect("Should accept empty"), dstack: vec![vec![2]], init: Default::default() },
            TestCase { code: opcodes::Op3::empty().expect("Should accept empty"), dstack: vec![vec![3]], init: Default::default() },
            TestCase { code: opcodes::Op4::empty().expect("Should accept empty"), dstack: vec![vec![4]], init: Default::default() },
            TestCase { code: opcodes::Op5::empty().expect("Should accept empty"), dstack: vec![vec![5]], init: Default::default() },
            TestCase { code: opcodes::Op6::empty().expect("Should accept empty"), dstack: vec![vec![6]], init: Default::default() },
            TestCase { code: opcodes::Op7::empty().expect("Should accept empty"), dstack: vec![vec![7]], init: Default::default() },
            TestCase { code: opcodes::Op8::empty().expect("Should accept empty"), dstack: vec![vec![8]], init: Default::default() },
            TestCase { code: opcodes::Op9::empty().expect("Should accept empty"), dstack: vec![vec![9]], init: Default::default() },
            TestCase { code: opcodes::Op10::empty().expect("Should accept empty"), dstack: vec![vec![10]], init: Default::default() },
            TestCase { code: opcodes::Op11::empty().expect("Should accept empty"), dstack: vec![vec![11]], init: Default::default() },
            TestCase { code: opcodes::Op12::empty().expect("Should accept empty"), dstack: vec![vec![12]], init: Default::default() },
            TestCase { code: opcodes::Op13::empty().expect("Should accept empty"), dstack: vec![vec![13]], init: Default::default() },
            TestCase { code: opcodes::Op14::empty().expect("Should accept empty"), dstack: vec![vec![14]], init: Default::default() },
            TestCase { code: opcodes::Op15::empty().expect("Should accept empty"), dstack: vec![vec![15]], init: Default::default() },
            TestCase { code: opcodes::Op16::empty().expect("Should accept empty"), dstack: vec![vec![16]], init: Default::default() },
        ]);
    }

    #[test]
    fn test_uniary_num_ops() {
        run_test_cases(vec![
            TestCase { code: opcodes::Op1Add::empty().expect("Should accept empty"), init: vec![vec![]], dstack: vec![vec![1]] },
            TestCase { code: opcodes::Op1Add::empty().expect("Should accept empty"), init: vec![vec![1]], dstack: vec![vec![2]] },
            TestCase {
                code: opcodes::Op1Add::empty().expect("Should accept empty"),
                init: vec![vec![2, 1]],
                dstack: vec![vec![3, 1]],
            },
            TestCase { code: opcodes::Op1Add::empty().expect("Should accept empty"), init: vec![vec![0x81]], dstack: vec![vec![]] },
            TestCase { code: opcodes::Op1Sub::empty().expect("Should accept empty"), init: vec![vec![]], dstack: vec![vec![0x81]] },
            TestCase { code: opcodes::Op1Sub::empty().expect("Should accept empty"), init: vec![vec![1]], dstack: vec![vec![]] },
            TestCase { code: opcodes::Op1Sub::empty().expect("Should accept empty"), init: vec![vec![2]], dstack: vec![vec![1]] },
            TestCase {
                code: opcodes::Op1Sub::empty().expect("Should accept empty"),
                init: vec![vec![3, 1]],
                dstack: vec![vec![2, 1]],
            },
            TestCase { code: opcodes::OpNegate::empty().expect("Should accept empty"), init: vec![vec![]], dstack: vec![vec![]] },
            TestCase { code: opcodes::OpNegate::empty().expect("Should accept empty"), init: vec![vec![1]], dstack: vec![vec![0x81]] },
            TestCase { code: opcodes::OpNegate::empty().expect("Should accept empty"), init: vec![vec![0x81]], dstack: vec![vec![1]] },
            TestCase {
                code: opcodes::OpNegate::empty().expect("Should accept empty"),
                init: vec![vec![3, 1]],
                dstack: vec![vec![3, 0x81]],
            },
            TestCase { code: opcodes::OpAbs::empty().expect("Should accept empty"), init: vec![vec![]], dstack: vec![vec![]] },
            TestCase { code: opcodes::OpAbs::empty().expect("Should accept empty"), init: vec![vec![3, 1]], dstack: vec![vec![3, 1]] },
            TestCase {
                code: opcodes::OpAbs::empty().expect("Should accept empty"),
                init: vec![vec![3, 0x81]],
                dstack: vec![vec![3, 1]],
            },
            TestCase { code: opcodes::OpAbs::empty().expect("Should accept empty"), init: vec![vec![1]], dstack: vec![vec![1]] },
            TestCase { code: opcodes::OpAbs::empty().expect("Should accept empty"), init: vec![vec![0x81]], dstack: vec![vec![1]] },
            TestCase {
                code: opcodes::OpAbs::empty().expect("Should accept empty"),
                init: vec![vec![1, 1, 0x82]],
                dstack: vec![vec![1, 1, 2]],
            },
            TestCase { code: opcodes::OpNot::empty().expect("Should accept empty"), init: vec![vec![]], dstack: vec![vec![1]] },
            TestCase { code: opcodes::OpNot::empty().expect("Should accept empty"), init: vec![vec![1]], dstack: vec![vec![]] },
            TestCase { code: opcodes::OpNot::empty().expect("Should accept empty"), init: vec![vec![1, 2, 3]], dstack: vec![vec![]] },
            TestCase { code: opcodes::Op0NotEqual::empty().expect("Should accept empty"), init: vec![vec![]], dstack: vec![vec![]] },
            TestCase { code: opcodes::Op0NotEqual::empty().expect("Should accept empty"), init: vec![vec![1]], dstack: vec![vec![1]] },
            TestCase { code: opcodes::Op0NotEqual::empty().expect("Should accept empty"), init: vec![vec![2]], dstack: vec![vec![1]] },
            TestCase {
                code: opcodes::Op0NotEqual::empty().expect("Should accept empty"),
                init: vec![vec![1, 2, 3]],
                dstack: vec![vec![1]],
            },
        ]);
    }

    #[test]
    fn test_binary_num_ops() {
        run_test_cases(vec![
            TestCase { code: opcodes::OpAdd::empty().expect("Should accept empty"), init: vec![vec![], vec![]], dstack: vec![vec![]] },
            TestCase {
                code: opcodes::OpAdd::empty().expect("Should accept empty"),
                init: vec![vec![], vec![1]],
                dstack: vec![vec![1]],
            },
            TestCase {
                code: opcodes::OpAdd::empty().expect("Should accept empty"),
                init: vec![vec![1], vec![]],
                dstack: vec![vec![1]],
            },
            TestCase {
                code: opcodes::OpAdd::empty().expect("Should accept empty"),
                init: vec![vec![1], vec![1]],
                dstack: vec![vec![2]],
            },
            TestCase {
                code: opcodes::OpAdd::empty().expect("Should accept empty"),
                init: vec![vec![0x81], vec![1]],
                dstack: vec![vec![]],
            },
            TestCase {
                code: opcodes::OpAdd::empty().expect("Should accept empty"),
                init: vec![vec![0x7f], vec![1]],
                dstack: vec![vec![0x80, 0]],
            },
            TestCase {
                code: opcodes::OpAdd::empty().expect("Should accept empty"),
                init: vec![vec![0x80, 0], vec![0x80, 0]],
                dstack: vec![vec![0, 1]],
            },
            TestCase {
                code: opcodes::OpAdd::empty().expect("Should accept empty"),
                init: vec![vec![0xff, 0], vec![1]],
                dstack: vec![vec![0, 1]],
            },
            TestCase { code: opcodes::OpSub::empty().expect("Should accept empty"), init: vec![vec![], vec![]], dstack: vec![vec![]] },
            TestCase {
                code: opcodes::OpSub::empty().expect("Should accept empty"),
                init: vec![vec![], vec![1]],
                dstack: vec![vec![0x81]],
            },
            TestCase {
                code: opcodes::OpSub::empty().expect("Should accept empty"),
                init: vec![vec![1], vec![]],
                dstack: vec![vec![1]],
            },
            TestCase {
                code: opcodes::OpSub::empty().expect("Should accept empty"),
                init: vec![vec![1], vec![1]],
                dstack: vec![vec![]],
            },
            TestCase {
                code: opcodes::OpSub::empty().expect("Should accept empty"),
                init: vec![vec![0x81], vec![1]],
                dstack: vec![vec![0x82]],
            },
            TestCase {
                code: opcodes::OpSub::empty().expect("Should accept empty"),
                init: vec![vec![0x80, 0], vec![1]],
                dstack: vec![vec![0x7f]],
            },
            TestCase {
                code: opcodes::OpSub::empty().expect("Should accept empty"),
                init: vec![vec![0, 1], vec![0x80, 0]],
                dstack: vec![vec![0x80, 0]],
            },
            TestCase {
                code: opcodes::OpSub::empty().expect("Should accept empty"),
                init: vec![vec![0, 1], vec![1]],
                dstack: vec![vec![0xff, 0]],
            },
        ]);
    }
}
