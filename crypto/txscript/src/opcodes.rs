#[macro_use]
mod macros;

use crate::{MAX_OPS_PER_SCRIPT, ScriptSource, Stack, TxScriptEngine, TxScriptError, SEQUENCE_LOCK_TIME_DISABLED, SEQUENCE_LOCK_TIME_MASK, LOCK_TIME_THRESHOLD, MAX_TX_IN_SEQUENCE_NUM, MAX_PUB_KEYS_PER_MUTLTISIG};
use blake2b_simd::blake2b;
use consensus_core::hashing::sighash_type::SigHashType;
use core::cmp::{max, min};
use core::iter;
use core::mem::size_of;
use sha2::{Digest, Sha256};
use std::fmt::{Debug, Formatter};

type OpCodeResult = Result<(), TxScriptError>;

pub(crate) struct OpCode<const CODE: u8> {
    data: Vec<u8>,
}

impl<const CODE: u8> Debug for OpCode<CODE> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Opcode<{:#2x}>{{ data:{:?} }}", CODE, self.data)
    }
}

pub trait OpCodeImplementation: Debug {
    fn execute(&self, vm: &mut TxScriptEngine) -> OpCodeResult;
    // TODO: could be implemented as staticmethod for all opcodes at once. Maybe need its own trait?
    // Opcode number
    fn value(&self) -> u8;
    // length of data
    fn len(&self) -> usize;
    // Conditional should be executed also is not in branch
    fn is_conditional(&self) -> bool;
    // For push data- check if we can use shorter encoding
    fn check_minimal_data_push(&self) -> Result<(), TxScriptError>;
}

trait DataStack {
    fn pop_item<const SIZE: usize, T: Debug>(&mut self) -> Result<[T; SIZE], TxScriptError>
    where
        Vec<u8>: OpcodeData<T>;
    fn last_item<const SIZE: usize, T: Debug>(&self) -> Result<[T; SIZE], TxScriptError>
    where
        Vec<u8>: OpcodeData<T>;
    fn pop_raw<const SIZE: usize>(&mut self) -> Result<[Vec<u8>; SIZE], TxScriptError>;
    fn last_raw<const SIZE: usize>(&self) -> Result<[Vec<u8>; SIZE], TxScriptError>;
    fn push_item<T: Debug>(&mut self, item: T)
    where
        Vec<u8>: OpcodeData<T>;
}

impl DataStack for Stack {
    #[inline]
    fn pop_item<const SIZE: usize, T: Debug>(&mut self) -> Result<[T; SIZE], TxScriptError>
    where
        Vec<u8>: OpcodeData<T>,
    {
        if self.len() < SIZE {
            return Err(TxScriptError::EmptyStack);
        }
        Ok(<[T; SIZE]>::try_from(self.split_off(self.len() - SIZE).iter().map(|v| v.deserialize()).collect::<Result<Vec<T>, _>>()?)
            .expect("Already exact item"))
    }

    #[inline]
    fn last_item<const SIZE: usize, T: Debug>(&self) -> Result<[T; SIZE], TxScriptError>
    where
        Vec<u8>: OpcodeData<T>,
    {
        if self.len() < SIZE {
            return Err(TxScriptError::EmptyStack);
        }
        Ok(<[T; SIZE]>::try_from(self[self.len() - SIZE..].iter().map(|v| v.deserialize()).collect::<Result<Vec<T>, _>>()?)
            .expect("Already exact item"))
    }

    #[inline]
    fn  pop_raw<const SIZE: usize>(&mut self) -> Result<[Vec<u8>; SIZE], TxScriptError> {
        if self.len() < SIZE {
            return Err(TxScriptError::EmptyStack);
        }
        Ok(<[Vec<u8>; SIZE]>::try_from(self.split_off(self.len() - SIZE)).expect("Already exact item"))
    }

    #[inline]
    fn last_raw<const SIZE: usize>(&self) -> Result<[Vec<u8>; SIZE], TxScriptError> {
        if self.len() < SIZE {
            return Err(TxScriptError::EmptyStack);
        }
        Ok(<[Vec<u8>; SIZE]>::try_from(self[self.len() - SIZE..].to_vec()).expect("Already exact item"))
    }

    #[inline]
    fn push_item<T: Debug>(&mut self, item: T)
    where
        Vec<u8>: OpcodeData<T>,
    {
        Vec::push(self, OpcodeData::serialize(&item));
    }

    /*#[inline]
    fn push(&mut self, item: Vec<u8>) {
        Vec::push(self, item);
    }*/
}

trait OpcodeData<T> {
    fn deserialize(&self) -> Result<T, TxScriptError>;
    fn serialize(from: &T) -> Self;
}

impl OpcodeData<i32> for Vec<u8> {
    #[inline]
    fn deserialize(&self) -> Result<i32, TxScriptError> {
        match self.len() {
            l if l > size_of::<i32>() => Err(TxScriptError::InvalidState("data is too big for `i32`".to_string())),
            l if l == 0 => Ok(0),
            _ => {
                let msb = self[self.len() - 1];
                let first_byte = ((msb & 0x7f) as i32) * (2 * ((msb >> 7) as i32) - 1);
                Ok(self.iter().rev().map(|v| *v as i32).fold(first_byte, |accum, item| (accum << size_of::<u8>()) + item))
            }
        }
    }

    #[inline]
    fn serialize(from: &i32) -> Self {
        let sign = from.signum();
        let mut positive = from.abs();
        let mut last_saturated = false;
        iter::from_fn(move || {
            if positive == 0 {
                if sign == 1 && last_saturated {
                    last_saturated = false;
                    Some(0)
                } else {
                    None
                }
            } else {
                let value = positive & 0xff;
                last_saturated = (value & 0x80) != 0;
                positive >>= 8;
                Some(value as u8)
            }
        })
        .collect()
    }
}

impl OpcodeData<bool> for Vec<u8> {
    #[inline]
    fn deserialize(&self) -> Result<bool, TxScriptError> {
        Ok(self[self.len() - 1] & 0x7f != 0x0 || self[..self.len() - 1].iter().any(|&b| b != 0x0))
    }

    #[inline]
    fn serialize(from: &bool) -> Self {
        match from {
            true => vec![1],
            false => vec![],
        }
    }
}

// Helpers for some opcodes with shared data
#[inline]
fn push_data(data: Vec<u8>, vm: &mut TxScriptEngine) -> OpCodeResult {
    vm.dstack.push(data);
    Ok(())
}

#[inline]
fn push_number(number: i64, vm: &mut TxScriptEngine) -> OpCodeResult {
    vm.dstack.push(number.to_le_bytes().to_vec());
    Ok(())
}

#[inline]
fn drop_n<const N: usize>(vm: &mut TxScriptEngine) -> OpCodeResult {
    match vm.dstack.len() >= N {
        true => {
            vm.dstack.truncate(vm.dstack.len() - N);
            Ok(())
        }
        false => Err(TxScriptError::EmptyStack),
    }
}

#[inline]
fn dup_n<const N: usize>(vm: &mut TxScriptEngine) -> OpCodeResult {
    match vm.dstack.len() >= N {
        true => {
            vm.dstack.extend_from_slice(vm.dstack.clone()[vm.dstack.len() - N..].iter().as_slice());
            Ok(())
        }
        false => Err(TxScriptError::EmptyStack),
    }
}

#[inline]
fn over_n<const N: usize>(vm: &mut TxScriptEngine) -> OpCodeResult {
    match vm.dstack.len() >= 2 * N {
        true => {
            vm.dstack.extend_from_slice(vm.dstack.clone()[vm.dstack.len() - 2 * N..vm.dstack.len() - N].iter().as_slice());
            Ok(())
        }
        false => Err(TxScriptError::EmptyStack),
    }
}

#[inline]
fn rot_n<const N: usize>(vm: &mut TxScriptEngine) -> OpCodeResult {
    match vm.dstack.len() >= 3 * N {
        true => {
            let drained = vm.dstack.drain(vm.dstack.len() - 3 * N..vm.dstack.len() - 2 * N).collect::<Vec<Vec<u8>>>();
            vm.dstack.extend(drained);
            Ok(())
        }
        false => Err(TxScriptError::EmptyStack),
    }
}

#[inline]
fn swap_n<const N: usize>(vm: &mut TxScriptEngine) -> OpCodeResult {
    match vm.dstack.len() >= 2 * N {
        true => {
            let drained = vm.dstack.drain(vm.dstack.len() - 2 * N..vm.dstack.len() - N).collect::<Vec<Vec<u8>>>();
            vm.dstack.extend(drained);
            Ok(())
        }
        false => Err(TxScriptError::EmptyStack),
    }
}

/*
The following is the implementation and metadata of all opcodes. Each opcode has unique
number (and templating system makes it impossible to use two opcodes), length specification,
and execution code.

The syntax is as follows:
```
opcode OpCodeName<id, length>(self, vm) {
    code;
    output
}
// OR
opcode OpCodeName<id, length>(self, vm) statement
```

Length specification is either a number (for fixed length) or a unsigned integer type
(for var length).
The execution code is implementing OpCodeImplementation. You can access the engime using the `vm`
variable.

Implementation details in `opcodes/macros.rs`.
*/
opcode_list! {

    // Data push opcodes.
    opcode OpFalse<0x00, 1>(self , vm) {
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

    opcode OpReserved<0x50, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{:?}", self)))

    opcode OpTrue<0x51, 1>(self, vm) push_number(1, vm)
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
    opcode OpVer<0x62, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{:?}", self)))

    opcode OpIf<0x63, 1>(self, vm) {
        let mut cond = 0;
        if vm.is_executing() {
            if let Some(mut cond_buf) = vm.dstack.pop() {
                if cond_buf.len() > 1 {
                    return Err(TxScriptError::InvalidState("expected boolean".to_string()));
                }
                cond = match cond_buf.pop() {
                    Some(stack_cond) => match stack_cond {
                        1 => 1,
                        _ => return Err(TxScriptError::InvalidState("expected boolean".to_string())),
                    }
                    None => -1,
                }
            } else {
                return Err(TxScriptError::EmptyStack);
            }
        }
        vm.cond_stack.push(cond);
        Ok(())
    }

    opcode OpNotIf<0x64, 1>(self, vm) {
        let mut cond = 0;
        if vm.is_executing() {
            if let Some(mut cond_buf) = vm.dstack.pop() {
                if cond_buf.len() > 1 {
                    return Err(TxScriptError::InvalidState("expected boolean".to_string()));
                }
                cond = match cond_buf.pop() {
                    Some(stack_cond) => match stack_cond {
                        1 => -1,
                        _ => return Err(TxScriptError::InvalidState("expected boolean".to_string())),
                    }
                    None => 1,
                }
            } else {
                return Err(TxScriptError::EmptyStack);
            }
        }
        vm.cond_stack.push(cond);
        Ok(())
    }

    opcode OpVerIf<0x65, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{:?}", self)))
    opcode OpVerNotIf<0x66, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{:?}", self)))

    opcode OpElse<0x67, 1>(self, vm) {
        if let Some(cond) = vm.cond_stack.last_mut() {
            *cond *= -1;
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
        let [result]: [bool; 1] = vm.dstack.pop_item()?;
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

    opcode Op2Drop<0x6d, 1>(self, vm) drop_n::<2>(vm)
    opcode Op2Dup<0x6e, 1>(self, vm) dup_n::<2>(vm)
    opcode Op3Dup<0x6f, 1>(self, vm) dup_n::<3>(vm)
    opcode Op2Over<0x70, 1>(self, vm) over_n::<2>(vm)
    opcode Op2Rot<0x71, 1>(self, vm) rot_n::<2>(vm)
    opcode Op2Swap<0x72, 1>(self, vm) swap_n::<2>(vm)

    opcode OpIfDup<0x73, 1>(self, vm) {
        let [result] = vm.dstack.pop_raw()?;
        if <Vec<u8> as OpcodeData<bool>>::deserialize(&result)? {
            vm.dstack.push(result);
        }
        Ok(())
    }

    opcode OpDepth<0x74, 1>(self, vm) push_number(vm.dstack.len() as i64, vm)

    opcode OpDrop<0x75, 1>(self, vm) drop_n::<1>(vm)
    opcode OpDup<0x76, 1>(self, vm) dup_n::<1>(vm)

    opcode OpNip<0x77, 1>(self, vm) {
        match vm.dstack.len() >= 2 {
            true => {
                vm.dstack.remove(vm.dstack.len()-2);
                Ok(())
            }
            false => Err(TxScriptError::EmptyStack),
        }
    }

    opcode OpOver<0x78, 1>(self, vm) over_n::<1>(vm)

    opcode OpPick<0x79, 1>(self, vm) {
        let [loc]: [i32; 1] = vm.dstack.pop_item()?;
        match loc >= 0 {
            true => {
                vm.dstack.push(vm.dstack[vm.dstack.len()-(loc as usize)-1].clone());
                Ok(())
            }
            false => Err(TxScriptError::InvalidState("pick at a negative value".to_string()))
        }
    }

    opcode OpRoll<0x7a, 1>(self, vm) {
        let [loc]: [i32; 1] = vm.dstack.pop_item()?;
        match loc >= 0 {
            true => {
                let item = vm.dstack.remove(vm.dstack.len()-(loc as usize)-1);
                vm.dstack.push(item);
                Ok(())
            }
            false => Err(TxScriptError::InvalidState("pick at a negative value".to_string()))
        }
    }

    opcode OpRot<0x7b, 1>(self, vm) rot_n::<1>(vm)
    opcode OpSwap<0x7c, 1>(self, vm) swap_n::<1>(vm)

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
    opcode OpCat<0x7e, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))
    opcode OpSubStr<0x7f, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))
    opcode OpLeft<0x80, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))
    opcode OpRight<0x81, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))

    opcode OpSize<0x82, 1>(self, vm) {
        match vm.dstack.last() {
            Some(last) => {
                //TODO: resolve the conflict between 32 bit and 64 bits
                vm.dstack.push_item(last.len() as i32);
                Ok(())
            },
            None => Err(TxScriptError::EmptyStack)
        }
    }

    // Bitwise logic opcodes.
    opcode OpInvert<0x83, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))
    opcode OpAnd<0x84, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))
    opcode OpOr<0x85, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))
    opcode OpXor<0x86, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))

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

    opcode OpReserved1<0x89, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{:?}", self)))
    opcode OpReserved2<0x8a, 1>(self, vm) Err(TxScriptError::OpcodeReserved(format!("{:?}", self)))

    // Numeric related opcodes.
    opcode Op1Add<0x8b, 1>(self, vm) {
        let [value]: [i32; 1] = vm.dstack.pop_item()?;
        vm.dstack.push_item(value + 1);
        Ok(())
    }

    opcode Op1Sub<0x8c, 1>(self, vm) {
        let [value]: [i32; 1] = vm.dstack.pop_item()?;
        vm.dstack.push_item(value - 1);
        Ok(())
    }

    opcode Op2Mul<0x8d, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))
    opcode Op2Div<0x8e, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))

    opcode OpNegate<0x8f, 1>(self, vm) {
        let [value]: [i32; 1] = vm.dstack.pop_item()?;
        vm.dstack.push_item(-value);
        Ok(())
    }

    opcode OpAbs<0x90, 1>(self, vm) {
        let [m]: [i32; 1] = vm.dstack.pop_item()?;
        vm.dstack.push_item(m.abs());
        Ok(())
    }

    opcode OpNot<0x91, 1>(self, vm) {
        let [m]: [i32; 1] = vm.dstack.pop_item()?;
        vm.dstack.push_item((m == 0) as i32);
        Ok(())
    }

    opcode Op0NotEqual<0x92, 1>(self, vm) {
        let [m]: [i32; 1] = vm.dstack.pop_item()?;
        vm.dstack.push_item((m != 0) as i32);
        Ok(())
    }

    opcode OpAdd<0x93, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item(a+b);
        Ok(())
    }

    opcode OpSub<0x94, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item(a-b);
        Ok(())
    }

    opcode OpMul<0x95, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))
    opcode OpDiv<0x96, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))
    opcode OpMod<0x97, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))
    opcode OpLShift<0x98, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))
    opcode OpRShift<0x99, 1>(self, vm) Err(TxScriptError::OpcodeDisabled(format!("{:?}", self)))

    opcode OpBoolAnd<0x9a, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item(((a != 0) && (b != 0)) as i32);
        Ok(())
    }

    opcode OpBoolOr<0x9b, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item(((a != 0) || (b != 0)) as i32);
        Ok(())
    }

    opcode OpNumEqual<0x9c, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item((a == b) as i32);
        Ok(())
    }

    opcode OpNumEqualVerify<0x9d, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        match a == b {
            true => Ok(()),
            false => Err(TxScriptError::VerifyError)
        }
    }

    opcode OpNumNotEqual<0x9e, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item((a != b) as i32);
        Ok(())
    }

    opcode OpLessThan<0x9f, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item((a < b) as i32);
        Ok(())
    }

    opcode OpGreaterThan<0xa0, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item((a > b) as i32);
        Ok(())
    }

    opcode OpLessThanOrEqual<0xa1, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item((a <= b) as i32);
        Ok(())
    }

    opcode OpGreaterThanOrEqual<0xa2, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item((a >= b) as i32);
        Ok(())
    }

    opcode OpMin<0xa3, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item(min(a,b));
        Ok(())
    }

    opcode OpMax<0xa4, 1>(self, vm) {
        let [a,b]: [i32; 2] = vm.dstack.pop_item()?;
        vm.dstack.push_item(max(a,b));
        Ok(())
    }

    opcode OpWithin<0xa5, 1>(self, vm) {
        let [x,l,u]: [i32; 3] = vm.dstack.pop_item()?;
        vm.dstack.push_item((x >= l && x < u) as i32);
        Ok(())
    }

    // Undefined opcodes.
    opcode OpUnknown166<0xa6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown167<0xa7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))

    // Crypto opcodes.
    opcode OpSHA256<0xa8, 1>(self, vm) {
        let [last] = vm.dstack.pop_raw()?;
        let mut hasher = Sha256::new();
        hasher.update(last);
        vm.dstack.push(hasher.finalize().to_vec());
        Ok(())
    }

    opcode OpCheckMultiSigECDSA<0xa9, 1>(self, vm) {
        todo!()
    }

    opcode OpBlake2b<0xaa, 1>(self, vm) {
        let [last] = vm.dstack.pop_raw()?;
        let hash = blake2b(last.as_slice());
        vm.dstack.push(hash.as_bytes().to_vec());
        Ok(())
    }

    opcode OpCheckSigECDSA<0xab, 1>(self, vm) {
        todo!()
    }

    opcode OpCheckSig<0xac, 1>(self, vm) {
        let [mut sig, key] = vm.dstack.pop_raw()?;
        // Hash type
        match sig.pop() {
            Some(typ) => {
                //TODO: check signature length (pair[0])
                //TODO: check public key encoding (pair[1])
                //TODO: calculate signature hash schnorr
                let hash_type = SigHashType::from_u8(typ).map_err(|e| TxScriptError::InvalidSigHashType(e.into()))?;
                match vm.check_schnorr_signature(hash_type, key.as_slice(), sig.as_slice()) {
                    Ok(()) => {
                        vm.dstack.push_item(true);
                        Ok(())
                    },
                    Err(e) => {
                        vm.dstack.push_item(false);
                        Ok(())
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
        // TODO: when chaging impl to array based, change this too
        OpCheckSig{data: self.data.clone()}.execute(vm)?;
        let [valid]: [bool; 1] = vm.dstack.pop_item()?;
        match valid {
            true => Ok(()),
            false => Err(TxScriptError::VerifyError)
        }
    }

    opcode OpCheckMultiSig<0xae, 1>(self, vm) {
        let [num_keys]: [i32; 1] = vm.dstack.pop_item()?;
        if num_keys < 0 {
            return Err(TxScriptError::InvalidPubKeyCount(format!("number of pubkeys {} is negative", num_keys)));
        } else if num_keys > MAX_PUB_KEYS_PER_MUTLTISIG {
            return Err(TxScriptError::InvalidPubKeyCount(format!("too many pubkeys {} > {}", num_keys, MAX_PUB_KEYS_PER_MUTLTISIG)));
        }
        let num_keys_usize = num_keys as usize;

        vm.num_ops += num_keys;
        if vm.num_ops > MAX_OPS_PER_SCRIPT {
            return Err(TxScriptError::TooManyOperations(MAX_OPS_PER_SCRIPT));
        }

        let mut pub_keys_vec = match vm.dstack.len() >= num_keys_usize {
            true => vm.dstack.split_off(vm.dstack.len() - num_keys_usize),
            false => return Err(TxScriptError::EmptyStack),
        };
        let mut pub_keys = pub_keys_vec.iter_mut();


        let [num_sigs]: [i32; 1] = vm.dstack.pop_item()?;
        if num_sigs < 0 {
            return Err(TxScriptError::InvalidSignatureCount(format!("number of signatures {} is negative", num_sigs)));
        } else if num_sigs > num_keys {
            return Err(TxScriptError::InvalidSignatureCount(format!("more signatures than pubkeys {} > {}", num_sigs, num_keys)));
        }
        let num_sigs_usize = num_sigs as usize;

        let mut  signatures_vec = match vm.dstack.len() >= num_sigs_usize {
            true => vm.dstack.split_off(vm.dstack.len() - num_sigs_usize),
            false => return Err(TxScriptError::EmptyStack),
        };
        let signatures = signatures_vec.iter_mut();

        let mut empty_sigs = 0usize;
        for (sig_idx, signature) in signatures.enumerate() {
            match signature.pop() {
                None => {
                    if empty_sigs != sig_idx {
                        return Err(TxScriptError::NullFail)
                    }
                    empty_sigs+=1;
                },
                Some(typ) => {
                    if empty_sigs == 0 {
                        // Every check consumes the public key
                        //TODO: check signature length (pair[0])
                        //TODO: check public key encoding (pair[1])
                        //TODO: calculate signature hash schnorr
                        let hash_type = SigHashType::from_u8(typ).map_err(|e| TxScriptError::InvalidSigHashType(e.into()))?;
                        while pub_keys.len() > num_sigs_usize - sig_idx && vm.check_schnorr_signature(hash_type, pub_keys.next().expect("Checked larger than 0").as_slice(), signature.as_slice()).is_err() {}
                    }
                    if empty_sigs > 0 || pub_keys.len() > num_sigs_usize - sig_idx {
                        return Err(TxScriptError::NullFail)
                    }
                }
            }
        }
        vm.dstack.push_item(empty_sigs == 0);
        Ok(())
    }

    opcode OpCheckMultiSigVerify<0xaf, 1>(self, vm) {
        // TODO: when chaging impl to array based, change this too
        OpCheckMultiSig{data: self.data.clone()}.execute(vm)?;
        let [valid]: [bool; 1] = vm.dstack.pop_item()?;
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
                    return Err(TxScriptError::NumberTooBig(format!("lockTime value represented as {:x?} is longer then 8 bytes", lock_time_bytes)))
                }
                lock_time_bytes.resize(8, 0);
                let stack_lock_time = u64::from_le_bytes(lock_time_bytes.try_into().expect("checked vector size"));

                // The lock time field of a transaction is either a DAA score at
	            // which the transaction is finalized or a timestamp depending on if the
	            // value is before the constants.LockTimeThreshold. When it is under the
	            // threshold it is a DAA score.
                if !(
                    (tx.tx.lock_time < LOCK_TIME_THRESHOLD && stack_lock_time < LOCK_TIME_THRESHOLD) ||
                    (tx.tx.lock_time >= LOCK_TIME_THRESHOLD && stack_lock_time >= LOCK_TIME_THRESHOLD)
                ){
                    return Err(TxScriptError::UnsatisfiedLockTime(format!("mismatched locktime types -- tx locktime {}, stack locktime {}", tx.tx.lock_time, stack_lock_time)))
                }

                if stack_lock_time > tx.tx.lock_time {
                    return Err(TxScriptError::UnsatisfiedLockTime(format!("locktime requirement not satisfied -- locktime is greater than the transaction locktime: {} > {}", stack_lock_time, tx.tx.lock_time)))
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
                    return Err(TxScriptError::NumberTooBig(format!("lockTime value represented as {:x?} is longer then 8 bytes", sequence_bytes)))
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
    opcode OpUnknown178<0xb2, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown179<0xb3, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown180<0xb4, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown181<0xb5, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown182<0xb6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown183<0xb7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown184<0xb8, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown185<0xb9, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown186<0xba, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown187<0xbb, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown188<0xbc, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown189<0xbd, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown190<0xbe, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown191<0xbf, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown192<0xc0, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown193<0xc1, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown194<0xc2, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown195<0xc3, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown196<0xc4, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown197<0xc5, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown198<0xc6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown199<0xc7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown200<0xc8, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown201<0xc9, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown202<0xca, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown203<0xcb, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown204<0xcc, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown205<0xcd, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown206<0xce, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown207<0xcf, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown208<0xd0, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown209<0xd1, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown210<0xd2, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown211<0xd3, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown212<0xd4, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown213<0xd5, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown214<0xd6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown215<0xd7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown216<0xd8, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown217<0xd9, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown218<0xda, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown219<0xdb, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown220<0xdc, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown221<0xdd, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown222<0xde, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown223<0xdf, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown224<0xe0, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown225<0xe1, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown226<0xe2, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown227<0xe3, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown228<0xe4, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown229<0xe5, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown230<0xe6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown231<0xe7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown232<0xe8, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown233<0xe9, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown234<0xea, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown235<0xeb, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown236<0xec, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown237<0xed, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown238<0xee, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown239<0xef, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown240<0xf0, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown241<0xf1, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown242<0xf2, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown243<0xf3, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown244<0xf4, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown245<0xf5, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown246<0xf6, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown247<0xf7, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown248<0xf8, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown249<0xf9, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))

    opcode OpSmallInteger<0xfa, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpPubKeys<0xfb, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpUnknown252<0xfc, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpPubKeyHash<0xfd, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpPubKey<0xfe, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
    opcode OpInvalidOpCode<0xff, 1>(self, vm) Err(TxScriptError::InvalidOpcode(format!("{:?}", self)))
}
