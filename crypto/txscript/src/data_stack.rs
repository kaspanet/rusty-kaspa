use core::fmt::Debug;
use crate::TxScriptError;
use core::mem::size_of;
use core::iter;

pub(crate) type Stack = Vec<Vec<u8>>;

pub(crate) trait DataStack {
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
    fn drop_item<const SIZE: usize>(&mut self) -> Result<(), TxScriptError>;
    fn dup_item<const SIZE: usize>(&mut self) -> Result<(), TxScriptError>;
    fn over_item<const SIZE: usize>(&mut self) -> Result<(), TxScriptError>;
    fn rot_item<const SIZE: usize>(&mut self) -> Result<(), TxScriptError>;
    fn swap_item<const SIZE: usize>(&mut self) -> Result<(), TxScriptError>;
}

pub(crate) trait OpcodeData<T> {
    fn deserialize(&self) -> Result<T, TxScriptError>;
    fn serialize(from: &T) -> Self;
}

impl OpcodeData<i64> for Vec<u8> {
    #[inline]
    fn deserialize(&self) -> Result<i64, TxScriptError> {
        match self.len() {
            l if l > size_of::<i64>() => Err(TxScriptError::InvalidState("data is too big for `i64`".to_string())),
            l if l == 0 => Ok(0),
            _ => {
                let msb = self[self.len() - 1];
                let first_byte = ((msb & 0x7f) as i64) * (1 - 2 * ((msb >> 7) as i64));
                Ok(self[..self.len() - 1].iter().rev().map(|v| *v as i64)
                    .fold(first_byte, |accum, item| (accum << 8) + item))
            }
        }
    }

    #[inline]
    fn serialize(from: &i64) -> Self {
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


impl OpcodeData<i32> for Vec<u8> {
    #[inline]
    fn deserialize(&self) -> Result<i32, TxScriptError> {
        let res = OpcodeData::<i64>::deserialize(self)?;
        i32::try_from(res.clamp(i32::MIN as i64, i32::MAX as i64)).map_err(|e| TxScriptError::InvalidState(format!("data is too big for `i32`: {}", e)))
    }

    #[inline]
    fn serialize(from: &i32) -> Self {
        OpcodeData::<i64>::serialize(&(*from as i64))
    }
}

impl OpcodeData<bool> for Vec<u8> {
    #[inline]
    fn deserialize(&self) -> Result<bool, TxScriptError> {
        if self.len() == 0 {
            Ok(false)
        } else {
            // Negative 0 is also considered false
            Ok(self[self.len() - 1] & 0x7f != 0x0 || self[..self.len() - 1].iter().any(|&b| b != 0x0))
        }
    }

    #[inline]
    fn serialize(from: &bool) -> Self {
        match from {
            true => vec![1],
            false => vec![],
        }
    }
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

    #[inline]
    fn drop_item<const SIZE: usize>(&mut self) -> Result<(), TxScriptError> {
        match self.len() >= SIZE {
            true => {
                self.truncate(self.len() - SIZE);
                Ok(())
            }
            false => Err(TxScriptError::EmptyStack),
        }
    }

    #[inline]
    fn dup_item<const SIZE: usize>(&mut self) -> Result<(), TxScriptError> {
        match self.len() >= SIZE {
            true => {
                self.extend_from_slice(self.clone()[self.len() - SIZE..].iter().as_slice());
                Ok(())
            }
            false => Err(TxScriptError::EmptyStack),
        }
    }

    #[inline]
    fn over_item<const SIZE: usize>(&mut self) -> Result<(), TxScriptError> {
        match self.len() >= 2 * SIZE {
            true => {
                self.extend_from_slice(self.clone()[self.len() - 2 * SIZE..self.len() - SIZE].iter().as_slice());
                Ok(())
            }
            false => Err(TxScriptError::EmptyStack),
        }
    }

    #[inline]
    fn rot_item<const SIZE: usize>(&mut self) -> Result<(), TxScriptError> {
        match self.len() >= 3 * SIZE {
            true => {
                let drained = self.drain(self.len() - 3 * SIZE..self.len() - 2 * SIZE).collect::<Vec<Vec<u8>>>();
                self.extend(drained);
                Ok(())
            }
            false => Err(TxScriptError::EmptyStack),
        }
    }

    #[inline]
    fn swap_item<const SIZE: usize>(&mut self) -> Result<(), TxScriptError> {
        match self.len() >= 2 * SIZE {
            true => {
                let drained = self.drain(self.len() - 2 * SIZE..self.len() - SIZE).collect::<Vec<Vec<u8>>>();
                self.extend(drained);
                Ok(())
            }
            false => Err(TxScriptError::EmptyStack),
        }
    }
}