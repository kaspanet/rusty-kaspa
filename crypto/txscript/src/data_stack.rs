use crate::TxScriptError;
use core::fmt::Debug;
use core::iter;
use kaspa_txscript_errors::SerializationError;
use std::cmp::Ordering;
use std::num::TryFromIntError;
use std::ops::Deref;

#[derive(PartialEq, Eq, Debug, Default, PartialOrd, Ord)]
pub(crate) struct SizedEncodeInt<const LEN: usize>(pub(crate) i64);

impl<const LEN: usize> From<i64> for SizedEncodeInt<LEN> {
    fn from(value: i64) -> Self {
        SizedEncodeInt(value)
    }
}

impl<const LEN: usize> From<i32> for SizedEncodeInt<LEN> {
    fn from(value: i32) -> Self {
        SizedEncodeInt(value as i64)
    }
}

impl<const LEN: usize> TryFrom<SizedEncodeInt<LEN>> for i32 {
    type Error = TryFromIntError;

    fn try_from(value: SizedEncodeInt<LEN>) -> Result<Self, Self::Error> {
        value.0.try_into()
    }
}

impl<const LEN: usize> PartialEq<i64> for SizedEncodeInt<LEN> {
    fn eq(&self, other: &i64) -> bool {
        self.0 == *other
    }
}

impl<const LEN: usize> PartialOrd<i64> for SizedEncodeInt<LEN> {
    fn partial_cmp(&self, other: &i64) -> Option<Ordering> {
        self.0.partial_cmp(other)
    }
}

impl<const LEN: usize> Deref for SizedEncodeInt<LEN> {
    type Target = i64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const LEN: usize> From<SizedEncodeInt<LEN>> for i64 {
    fn from(value: SizedEncodeInt<LEN>) -> Self {
        value.0
    }
}

pub(crate) type Stack = Vec<Vec<u8>>;

pub(crate) trait DataStack {
    fn pop_items<const SIZE: usize, T: Debug>(&mut self) -> Result<[T; SIZE], TxScriptError>
    where
        Vec<u8>: OpcodeData<T>;
    #[allow(dead_code)]
    fn peek_items<const SIZE: usize, T: Debug>(&self) -> Result<[T; SIZE], TxScriptError>
    where
        Vec<u8>: OpcodeData<T>;
    fn pop_raw<const SIZE: usize>(&mut self) -> Result<[Vec<u8>; SIZE], TxScriptError>;
    fn peek_raw<const SIZE: usize>(&self) -> Result<[Vec<u8>; SIZE], TxScriptError>;
    fn push_item<T: Debug>(&mut self, item: T) -> Result<(), TxScriptError>
    where
        Vec<u8>: OpcodeData<T>;
    fn drop_items<const SIZE: usize>(&mut self) -> Result<(), TxScriptError>;
    fn dup_items<const SIZE: usize>(&mut self) -> Result<(), TxScriptError>;
    fn over_items<const SIZE: usize>(&mut self) -> Result<(), TxScriptError>;
    fn rot_items<const SIZE: usize>(&mut self) -> Result<(), TxScriptError>;
    fn swap_items<const SIZE: usize>(&mut self) -> Result<(), TxScriptError>;
}

pub(crate) trait OpcodeData<T> {
    fn deserialize(&self) -> Result<T, TxScriptError>;
    fn serialize(from: &T) -> Result<Self, SerializationError>
    where
        Self: Sized;
}

fn check_minimal_data_encoding(v: &[u8]) -> Result<(), TxScriptError> {
    if v.is_empty() {
        return Ok(());
    }

    // Check that the number is encoded with the minimum possible
    // number of bytes.
    //
    // If the most-significant-byte - excluding the sign bit - is zero
    // then we're not minimal. Note how this test also rejects the
    // negative-zero encoding, [0x80].
    if v[v.len() - 1] & 0x7f == 0 {
        // One exception: if there's more than one byte and the most
        // significant bit of the second-most-significant-byte is set
        // it would conflict with the sign bit. An example of this case
        // is +-255, which encode to 0xff00 and 0xff80 respectively.
        // (big-endian).
        if v.len() == 1 || v[v.len() - 2] & 0x80 == 0 {
            return Err(TxScriptError::NotMinimalData(format!("numeric value encoded as {v:x?} is not minimally encoded")));
        }
    }

    Ok(())
}

#[inline]
fn serialize_i64(from: &i64) -> Vec<u8> {
    let sign = from.signum();
    let mut positive = from.unsigned_abs();
    let mut last_saturated = false;
    let mut number_vec: Vec<u8> = iter::from_fn(move || {
        if positive == 0 {
            if last_saturated {
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
    .collect();
    if sign == -1 {
        match number_vec.last_mut() {
            Some(num) => *num |= 0x80,
            _ => unreachable!(),
        }
    }
    number_vec
}

fn deserialize_i64(v: &[u8]) -> Result<i64, TxScriptError> {
    match v.len() {
        l if l > size_of::<i64>() => {
            Err(TxScriptError::NotMinimalData(format!("numeric value encoded as {v:x?} is longer than 8 bytes")))
        }
        0 => Ok(0),
        _ => {
            check_minimal_data_encoding(v)?;
            let msb = v[v.len() - 1];
            let sign = 1 - 2 * ((msb >> 7) as i64);
            let first_byte = (msb & 0x7f) as i64;
            Ok(v[..v.len() - 1].iter().rev().map(|v| *v as i64).fold(first_byte, |accum, item| (accum << 8) + item) * sign)
        }
    }
}

// TODO: Rename to DefaultSizedEncodeInt when KIP-10 is activated
pub type Kip10I64 = SizedEncodeInt<8>;

impl OpcodeData<i64> for Vec<u8> {
    #[inline]
    fn deserialize(&self) -> Result<i64, TxScriptError> {
        // TODO: Change LEN to 8 once KIP-10 is activated
        OpcodeData::<SizedEncodeInt<4>>::deserialize(self).map(i64::from)
    }

    #[inline]
    fn serialize(from: &i64) -> Result<Self, SerializationError> {
        // Note that serialization and deserialization use different LEN.
        // This is because prior to KIP-10, only deserialization size was limited.
        // It's safe to use 8 here because i32 arithmetic operations (which were the
        // only ones that were supported prior to KIP-10) can't get to i64::MIN
        // (the only i64 value that requires more than 8 bytes to serialize).
        OpcodeData::<SizedEncodeInt<8>>::serialize(&(*from).into())
    }
}

impl OpcodeData<i32> for Vec<u8> {
    #[inline]
    fn deserialize(&self) -> Result<i32, TxScriptError> {
        OpcodeData::<SizedEncodeInt<4>>::deserialize(self).map(|v| v.try_into().expect("number is within i32 range"))
    }

    #[inline]
    fn serialize(from: &i32) -> Result<Self, SerializationError> {
        OpcodeData::<SizedEncodeInt<4>>::serialize(&(*from).into())
    }
}

impl<const LEN: usize> OpcodeData<SizedEncodeInt<LEN>> for Vec<u8> {
    #[inline]
    fn deserialize(&self) -> Result<SizedEncodeInt<LEN>, TxScriptError> {
        match self.len() > LEN {
            true => Err(TxScriptError::NumberTooBig(format!(
                "numeric value encoded as {:x?} is {} bytes which exceeds the max allowed of {}",
                self,
                self.len(),
                LEN
            ))),
            false => deserialize_i64(self).map(SizedEncodeInt::<LEN>),
        }
    }

    #[inline]
    fn serialize(from: &SizedEncodeInt<LEN>) -> Result<Self, SerializationError> {
        let bytes = serialize_i64(&from.0);
        if bytes.len() > LEN {
            return Err(SerializationError::NumberTooLong(from.0));
        }
        Ok(bytes)
    }
}

impl OpcodeData<bool> for Vec<u8> {
    #[inline]
    fn deserialize(&self) -> Result<bool, TxScriptError> {
        if self.is_empty() {
            Ok(false)
        } else {
            // Negative 0 is also considered false
            Ok(self[self.len() - 1] & 0x7f != 0x0 || self[..self.len() - 1].iter().any(|&b| b != 0x0))
        }
    }

    #[inline]
    fn serialize(from: &bool) -> Result<Self, SerializationError> {
        Ok(match from {
            true => vec![1],
            false => vec![],
        })
    }
}

impl DataStack for Stack {
    #[inline]
    fn pop_items<const SIZE: usize, T: Debug>(&mut self) -> Result<[T; SIZE], TxScriptError>
    where
        Vec<u8>: OpcodeData<T>,
    {
        if self.len() < SIZE {
            return Err(TxScriptError::InvalidStackOperation(SIZE, self.len()));
        }
        Ok(<[T; SIZE]>::try_from(self.split_off(self.len() - SIZE).iter().map(|v| v.deserialize()).collect::<Result<Vec<T>, _>>()?)
            .expect("Already exact item"))
    }

    #[inline]
    fn peek_items<const SIZE: usize, T: Debug>(&self) -> Result<[T; SIZE], TxScriptError>
    where
        Vec<u8>: OpcodeData<T>,
    {
        if self.len() < SIZE {
            return Err(TxScriptError::InvalidStackOperation(SIZE, self.len()));
        }
        Ok(<[T; SIZE]>::try_from(self[self.len() - SIZE..].iter().map(|v| v.deserialize()).collect::<Result<Vec<T>, _>>()?)
            .expect("Already exact item"))
    }

    #[inline]
    fn pop_raw<const SIZE: usize>(&mut self) -> Result<[Vec<u8>; SIZE], TxScriptError> {
        if self.len() < SIZE {
            return Err(TxScriptError::InvalidStackOperation(SIZE, self.len()));
        }
        Ok(<[Vec<u8>; SIZE]>::try_from(self.split_off(self.len() - SIZE)).expect("Already exact item"))
    }

    #[inline]
    fn peek_raw<const SIZE: usize>(&self) -> Result<[Vec<u8>; SIZE], TxScriptError> {
        if self.len() < SIZE {
            return Err(TxScriptError::InvalidStackOperation(SIZE, self.len()));
        }
        Ok(<[Vec<u8>; SIZE]>::try_from(self[self.len() - SIZE..].to_vec()).expect("Already exact item"))
    }

    #[inline]
    fn push_item<T: Debug>(&mut self, item: T) -> Result<(), TxScriptError>
    where
        Vec<u8>: OpcodeData<T>,
    {
        let v = OpcodeData::serialize(&item)?;
        Vec::push(self, v);
        Ok(())
    }

    #[inline]
    fn drop_items<const SIZE: usize>(&mut self) -> Result<(), TxScriptError> {
        match self.len() >= SIZE {
            true => {
                self.truncate(self.len() - SIZE);
                Ok(())
            }
            false => Err(TxScriptError::InvalidStackOperation(SIZE, self.len())),
        }
    }

    #[inline]
    fn dup_items<const SIZE: usize>(&mut self) -> Result<(), TxScriptError> {
        match self.len() >= SIZE {
            true => {
                self.extend_from_within(self.len() - SIZE..);
                Ok(())
            }
            false => Err(TxScriptError::InvalidStackOperation(SIZE, self.len())),
        }
    }

    #[inline]
    fn over_items<const SIZE: usize>(&mut self) -> Result<(), TxScriptError> {
        match self.len() >= 2 * SIZE {
            true => {
                self.extend_from_within(self.len() - 2 * SIZE..self.len() - SIZE);
                Ok(())
            }
            false => Err(TxScriptError::InvalidStackOperation(2 * SIZE, self.len())),
        }
    }

    #[inline]
    fn rot_items<const SIZE: usize>(&mut self) -> Result<(), TxScriptError> {
        match self.len() >= 3 * SIZE {
            true => {
                let drained = self.drain(self.len() - 3 * SIZE..self.len() - 2 * SIZE).collect::<Vec<Vec<u8>>>();
                self.extend(drained);
                Ok(())
            }
            false => Err(TxScriptError::InvalidStackOperation(3 * SIZE, self.len())),
        }
    }

    #[inline]
    fn swap_items<const SIZE: usize>(&mut self) -> Result<(), TxScriptError> {
        match self.len() >= 2 * SIZE {
            true => {
                let drained = self.drain(self.len() - 2 * SIZE..self.len() - SIZE).collect::<Vec<Vec<u8>>>();
                self.extend(drained);
                Ok(())
            }
            false => Err(TxScriptError::InvalidStackOperation(2 * SIZE, self.len())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Kip10I64, OpcodeData};
    use crate::data_stack::SizedEncodeInt;
    use kaspa_txscript_errors::{SerializationError, TxScriptError};

    // TestScriptNumBytes
    #[test]
    fn test_serialize() {
        struct TestCase {
            num: i64,
            serialized: Vec<u8>,
        }

        let tests = vec![
            TestCase { num: 0, serialized: vec![] },
            TestCase { num: 1, serialized: hex::decode("01").expect("failed parsing hex") },
            TestCase { num: -1, serialized: hex::decode("81").expect("failed parsing hex") },
            TestCase { num: 127, serialized: hex::decode("7f").expect("failed parsing hex") },
            TestCase { num: -127, serialized: hex::decode("ff").expect("failed parsing hex") },
            TestCase { num: 128, serialized: hex::decode("8000").expect("failed parsing hex") },
            TestCase { num: -128, serialized: hex::decode("8080").expect("failed parsing hex") },
            TestCase { num: 129, serialized: hex::decode("8100").expect("failed parsing hex") },
            TestCase { num: -129, serialized: hex::decode("8180").expect("failed parsing hex") },
            TestCase { num: 256, serialized: hex::decode("0001").expect("failed parsing hex") },
            TestCase { num: -256, serialized: hex::decode("0081").expect("failed parsing hex") },
            TestCase { num: 32767, serialized: hex::decode("ff7f").expect("failed parsing hex") },
            TestCase { num: -32767, serialized: hex::decode("ffff").expect("failed parsing hex") },
            TestCase { num: 32768, serialized: hex::decode("008000").expect("failed parsing hex") },
            TestCase { num: -32768, serialized: hex::decode("008080").expect("failed parsing hex") },
            TestCase { num: 65535, serialized: hex::decode("ffff00").expect("failed parsing hex") },
            TestCase { num: -65535, serialized: hex::decode("ffff80").expect("failed parsing hex") },
            TestCase { num: 524288, serialized: hex::decode("000008").expect("failed parsing hex") },
            TestCase { num: -524288, serialized: hex::decode("000088").expect("failed parsing hex") },
            TestCase { num: 7340032, serialized: hex::decode("000070").expect("failed parsing hex") },
            TestCase { num: -7340032, serialized: hex::decode("0000f0").expect("failed parsing hex") },
            TestCase { num: 8388608, serialized: hex::decode("00008000").expect("failed parsing hex") },
            TestCase { num: -8388608, serialized: hex::decode("00008080").expect("failed parsing hex") },
            TestCase { num: 2147483647, serialized: hex::decode("ffffff7f").expect("failed parsing hex") },
            TestCase { num: -2147483647, serialized: hex::decode("ffffffff").expect("failed parsing hex") },
            // Values that are out of range for data that is interpreted as
            // numbers before KIP-10 enabled, but are allowed as the result of numeric operations.
            TestCase { num: 2147483648, serialized: hex::decode("0000008000").expect("failed parsing hex") },
            TestCase { num: -2147483648, serialized: hex::decode("0000008080").expect("failed parsing hex") },
            TestCase { num: 2415919104, serialized: hex::decode("0000009000").expect("failed parsing hex") },
            TestCase { num: -2415919104, serialized: hex::decode("0000009080").expect("failed parsing hex") },
            TestCase { num: 4294967295, serialized: hex::decode("ffffffff00").expect("failed parsing hex") },
            TestCase { num: -4294967295, serialized: hex::decode("ffffffff80").expect("failed parsing hex") },
            TestCase { num: 4294967296, serialized: hex::decode("0000000001").expect("failed parsing hex") },
            TestCase { num: -4294967296, serialized: hex::decode("0000000081").expect("failed parsing hex") },
            TestCase { num: 281474976710655, serialized: hex::decode("ffffffffffff00").expect("failed parsing hex") },
            TestCase { num: -281474976710655, serialized: hex::decode("ffffffffffff80").expect("failed parsing hex") },
            TestCase { num: 72057594037927935, serialized: hex::decode("ffffffffffffff00").expect("failed parsing hex") },
            TestCase { num: -72057594037927935, serialized: hex::decode("ffffffffffffff80").expect("failed parsing hex") },
            TestCase { num: 9223372036854775807, serialized: hex::decode("ffffffffffffff7f").expect("failed parsing hex") },
            TestCase { num: -9223372036854775807, serialized: hex::decode("ffffffffffffffff").expect("failed parsing hex") },
        ];

        for test in tests {
            let serialized: Vec<u8> = OpcodeData::<i64>::serialize(&test.num).unwrap();
            assert_eq!(serialized, test.serialized);
        }

        // special case 9-byte i64
        let r: Result<Vec<u8>, _> = OpcodeData::<i64>::serialize(&-9223372036854775808);
        assert_eq!(r, Err(SerializationError::NumberTooLong(-9223372036854775808)));
    }

    // TestMakeScriptNum
    #[test]
    fn test_deserialize() {
        struct TestCase<T> {
            serialized: Vec<u8>,
            result: Result<T, TxScriptError>,
        }

        let tests = vec![
            TestCase::<i64> {
                serialized: hex::decode("80").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData("numeric value encoded as [80] is not minimally encoded".to_string())),
            },
            // Minimally encoded valid values with minimal encoding flag.
            // Should not error and return expected integral number.
            TestCase::<i64> { serialized: vec![], result: Ok(0) },
            TestCase::<i64> { serialized: hex::decode("01").expect("failed parsing hex"), result: Ok(1) },
            TestCase::<i64> { serialized: hex::decode("81").expect("failed parsing hex"), result: Ok(-1) },
            TestCase::<i64> { serialized: hex::decode("7f").expect("failed parsing hex"), result: Ok(127) },
            TestCase::<i64> { serialized: hex::decode("ff").expect("failed parsing hex"), result: Ok(-127) },
            TestCase::<i64> { serialized: hex::decode("8000").expect("failed parsing hex"), result: Ok(128) },
            TestCase::<i64> { serialized: hex::decode("8080").expect("failed parsing hex"), result: Ok(-128) },
            TestCase::<i64> { serialized: hex::decode("8100").expect("failed parsing hex"), result: Ok(129) },
            TestCase::<i64> { serialized: hex::decode("8180").expect("failed parsing hex"), result: Ok(-129) },
            TestCase::<i64> { serialized: hex::decode("0001").expect("failed parsing hex"), result: Ok(256) },
            TestCase::<i64> { serialized: hex::decode("0081").expect("failed parsing hex"), result: Ok(-256) },
            TestCase::<i64> { serialized: hex::decode("ff7f").expect("failed parsing hex"), result: Ok(32767) },
            TestCase::<i64> { serialized: hex::decode("ffff").expect("failed parsing hex"), result: Ok(-32767) },
            TestCase::<i64> { serialized: hex::decode("008000").expect("failed parsing hex"), result: Ok(32768) },
            TestCase::<i64> { serialized: hex::decode("008080").expect("failed parsing hex"), result: Ok(-32768) },
            TestCase::<i64> { serialized: hex::decode("ffff00").expect("failed parsing hex"), result: Ok(65535) },
            TestCase::<i64> { serialized: hex::decode("ffff80").expect("failed parsing hex"), result: Ok(-65535) },
            TestCase::<i64> { serialized: hex::decode("000008").expect("failed parsing hex"), result: Ok(524288) },
            TestCase::<i64> { serialized: hex::decode("000088").expect("failed parsing hex"), result: Ok(-524288) },
            TestCase::<i64> { serialized: hex::decode("000070").expect("failed parsing hex"), result: Ok(7340032) },
            TestCase::<i64> { serialized: hex::decode("0000f0").expect("failed parsing hex"), result: Ok(-7340032) },
            TestCase::<i64> { serialized: hex::decode("00008000").expect("failed parsing hex"), result: Ok(8388608) },
            TestCase::<i64> { serialized: hex::decode("00008080").expect("failed parsing hex"), result: Ok(-8388608) },
            TestCase::<i64> { serialized: hex::decode("ffffff7f").expect("failed parsing hex"), result: Ok(2147483647) },
            TestCase::<i64> { serialized: hex::decode("ffffffff").expect("failed parsing hex"), result: Ok(-2147483647) },
            /*
            TestCase::<i64>{serialized: hex::decode("ffffffffffffff7f").expect("failed parsing hex"), num_len: 8, result: Ok(9223372036854775807)},
            TestCase::<i64>{serialized: hex::decode("ffffffffffffffff").expect("failed parsing hex"), num_len: 8, result: Ok(-9223372036854775807)},*/
            // Minimally encoded values that are out of range for data that
            // is interpreted as script numbers with the minimal encoding
            // flag set. Should error and return 0.
            TestCase::<i64> {
                serialized: hex::decode("0000008000").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [0, 0, 0, 80, 0] is 5 bytes which exceeds the max allowed of 4".to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("0000008080").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [0, 0, 0, 80, 80] is 5 bytes which exceeds the max allowed of 4".to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("0000009000").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [0, 0, 0, 90, 0] is 5 bytes which exceeds the max allowed of 4".to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("0000009080").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [0, 0, 0, 90, 80] is 5 bytes which exceeds the max allowed of 4".to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("ffffffff00").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [ff, ff, ff, ff, 0] is 5 bytes which exceeds the max allowed of 4".to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("ffffffff80").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [ff, ff, ff, ff, 80] is 5 bytes which exceeds the max allowed of 4".to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("0000000001").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [0, 0, 0, 0, 1] is 5 bytes which exceeds the max allowed of 4".to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("0000000081").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [0, 0, 0, 0, 81] is 5 bytes which exceeds the max allowed of 4".to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("ffffffffffff00").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [ff, ff, ff, ff, ff, ff, 0] is 7 bytes which exceeds the max allowed of 4".to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("ffffffffffff80").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [ff, ff, ff, ff, ff, ff, 80] is 7 bytes which exceeds the max allowed of 4".to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("ffffffffffffff00").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [ff, ff, ff, ff, ff, ff, ff, 0] is 8 bytes which exceeds the max allowed of 4"
                        .to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("ffffffffffffff80").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [ff, ff, ff, ff, ff, ff, ff, 80] is 8 bytes which exceeds the max allowed of 4"
                        .to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("ffffffffffffff7f").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [ff, ff, ff, ff, ff, ff, ff, 7f] is 8 bytes which exceeds the max allowed of 4"
                        .to_string(),
                )),
            },
            TestCase::<i64> {
                serialized: hex::decode("ffffffffffffffff").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [ff, ff, ff, ff, ff, ff, ff, ff] is 8 bytes which exceeds the max allowed of 4"
                        .to_string(),
                )),
            },
            // Non-minimally encoded, but otherwise valid values with
            // minimal encoding flag. Should error and return 0.
            TestCase::<i64> {
                serialized: hex::decode("00").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData("numeric value encoded as [0] is not minimally encoded".to_string())),
            }, // 0
            TestCase::<i64> {
                serialized: hex::decode("0100").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData("numeric value encoded as [1, 0] is not minimally encoded".to_string())),
            }, // 1
            TestCase::<i64> {
                serialized: hex::decode("7f00").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData("numeric value encoded as [7f, 0] is not minimally encoded".to_string())),
            }, // 127
            TestCase::<i64> {
                serialized: hex::decode("800000").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData("numeric value encoded as [80, 0, 0] is not minimally encoded".to_string())),
            }, // 128
            TestCase::<i64> {
                serialized: hex::decode("810000").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData("numeric value encoded as [81, 0, 0] is not minimally encoded".to_string())),
            }, // 129
            TestCase::<i64> {
                serialized: hex::decode("000100").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData("numeric value encoded as [0, 1, 0] is not minimally encoded".to_string())),
            }, // 256
            TestCase::<i64> {
                serialized: hex::decode("ff7f00").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData(
                    "numeric value encoded as [ff, 7f, 0] is not minimally encoded".to_string(),
                )),
            }, // 32767
            TestCase::<i64> {
                serialized: hex::decode("00800000").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData(
                    "numeric value encoded as [0, 80, 0, 0] is not minimally encoded".to_string(),
                )),
            }, // 32768
            TestCase::<i64> {
                serialized: hex::decode("ffff0000").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData(
                    "numeric value encoded as [ff, ff, 0, 0] is not minimally encoded".to_string(),
                )),
            }, // 65535
            TestCase::<i64> {
                serialized: hex::decode("00000800").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData(
                    "numeric value encoded as [0, 0, 8, 0] is not minimally encoded".to_string(),
                )),
            }, // 524288
            TestCase::<i64> {
                serialized: hex::decode("00007000").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData(
                    "numeric value encoded as [0, 0, 70, 0] is not minimally encoded".to_string(),
                )),
            }, // 7340032
               // Values above 8 bytes should always return error
        ];
        let kip10_tests = vec![
            TestCase::<Kip10I64> {
                serialized: hex::decode("0000008000").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(2147483648i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("0000008080").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(-2147483648i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("0000009000").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(2415919104i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("0000009080").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(-2415919104i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("ffffffff00").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(4294967295i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("ffffffff80").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(-4294967295i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("0000000001").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(4294967296i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("0000000081").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(-4294967296i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("ffffffffffff00").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(281474976710655i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("ffffffffffff80").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(-281474976710655i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("ffffffffffffff00").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(72057594037927935i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("ffffffffffffff80").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(-72057594037927935i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("ffffffffffffff7f").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(9223372036854775807i64)),
            },
            TestCase::<Kip10I64> {
                serialized: hex::decode("ffffffffffffffff").expect("failed parsing hex"),
                result: Ok(Kip10I64::from(-9223372036854775807i64)),
            },
            // Minimally encoded values that are out of range for data that
            // is interpreted as script numbers with the minimal encoding
            // flag set. Should error and return 0.
            TestCase::<Kip10I64> {
                serialized: hex::decode("000000000000008080").expect("failed parsing hex"),
                result: Err(TxScriptError::NumberTooBig(
                    "numeric value encoded as [0, 0, 0, 0, 0, 0, 0, 80, 80] is 9 bytes which exceeds the max allowed of 8".to_string(),
                )),
            },
        ];
        let test_of_size_5 = vec![
            TestCase::<SizedEncodeInt<5>> {
                serialized: hex::decode("ffffffff7f").expect("failed parsing hex"),
                result: Ok(SizedEncodeInt::<5>(549755813887)),
            },
            TestCase::<SizedEncodeInt<5>> {
                serialized: hex::decode("ffffffffff").expect("failed parsing hex"),
                result: Ok(SizedEncodeInt::<5>(-549755813887)),
            },
            TestCase::<SizedEncodeInt<5>> {
                serialized: hex::decode("0009000100").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData(
                    "numeric value encoded as [0, 9, 0, 1, 0] is not minimally encoded".to_string(),
                )),
            }, // 16779520
        ];

        let test_of_size_8 = vec![
            TestCase::<SizedEncodeInt<8>> {
                serialized: hex::decode("ffffffffffffff7f").expect("failed parsing hex"),
                result: Ok(SizedEncodeInt::<8>(i64::MAX)),
            },
            TestCase::<SizedEncodeInt<8>> {
                serialized: hex::decode("ffffffffffffffff").expect("failed parsing hex"),
                result: Ok(SizedEncodeInt::<8>(i64::MIN + 1)),
            },
        ];

        let test_of_size_9 = vec![
            TestCase::<SizedEncodeInt<9>> {
                serialized: hex::decode("ffffffffffffffffff").expect("failed parsing hex"),
                result: Err(TxScriptError::NotMinimalData(
                    "numeric value encoded as [ff, ff, ff, ff, ff, ff, ff, ff, ff] is longer than 8 bytes".to_string(),
                )),
            },
            TestCase::<SizedEncodeInt<9>> {
                serialized: hex::decode("ffffffffffffffff").expect("failed parsing hex"),
                result: Ok(SizedEncodeInt::<9>(i64::MIN + 1)),
            },
        ];

        let test_of_size_10 = vec![TestCase::<SizedEncodeInt<10>> {
            serialized: hex::decode("00000000000000000000").expect("failed parsing hex"),
            result: Err(TxScriptError::NotMinimalData(
                "numeric value encoded as [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] is longer than 8 bytes".to_string(),
            )),
        }];

        let test_bool = vec![
            TestCase::<bool> { serialized: hex::decode("").expect("failed parsing hex"), result: Ok(false) },
            TestCase::<bool> { serialized: hex::decode("00").expect("failed parsing hex"), result: Ok(false) },
            TestCase::<bool> { serialized: hex::decode("0000").expect("failed parsing hex"), result: Ok(false) },
            TestCase::<bool> { serialized: hex::decode("0011").expect("failed parsing hex"), result: Ok(true) },
            TestCase::<bool> { serialized: hex::decode("80").expect("failed parsing hex"), result: Ok(false) }, // Negative zero
            TestCase::<bool> { serialized: hex::decode("8011").expect("failed parsing hex"), result: Ok(true) }, // MSB by itself is negative zero, but the whole number isn't
            TestCase::<bool> { serialized: hex::decode("8080").expect("failed parsing hex"), result: Ok(true) }, // All bytes are negative zeroes by themselves, but the whole number isn't
            TestCase::<bool> { serialized: hex::decode("1234").expect("failed parsing hex"), result: Ok(true) },
            TestCase::<bool> { serialized: hex::decode("ffffffff").expect("failed parsing hex"), result: Ok(true) },
        ];

        for test in tests {
            // Ensure the error code is of the expected type and the error
            // code matches the value specified in the test instance.
            assert_eq!(test.serialized.deserialize(), test.result);
        }

        for test in test_of_size_5 {
            // Ensure the error code is of the expected type and the error
            // code matches the value specified in the test instance.
            assert_eq!(test.serialized.deserialize(), test.result);
        }

        for test in test_of_size_8 {
            // Ensure the error code is of the expected type and the error
            // code matches the value specified in the test instance.
            assert_eq!(test.serialized.deserialize(), test.result);
        }

        for test in test_of_size_9 {
            // Ensure the error code is of the expected type and the error
            // code matches the value specified in the test instance.
            assert_eq!(test.serialized.deserialize(), test.result);
        }

        for test in test_of_size_10 {
            // Ensure the error code is of the expected type and the error
            // code matches the value specified in the test instance.
            assert_eq!(test.serialized.deserialize(), test.result);
        }

        for test in test_bool {
            // Ensure the error code is of the expected type and the error
            // code matches the value specified in the test instance.
            assert_eq!(test.serialized.deserialize(), test.result);
        }
        for test in kip10_tests {
            // Ensure the error code is of the expected type and the error
            // code matches the value specified in the test instance.
            assert_eq!(test.serialized.deserialize(), test.result);
        }
    }
}
