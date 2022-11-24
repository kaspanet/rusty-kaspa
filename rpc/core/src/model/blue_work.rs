extern crate derive_more;

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use consensus_core::BlueWorkType;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt::{Debug, Display, Formatter};
use std::str::{self, FromStr};

use crate::RpcError;

/// A RPC-dedicated representation of BlueWorkType in which string representation is always hexadecimal.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase", try_from = "String", into = "String")]
pub struct RpcBlueWorkType(BlueWorkType);

impl Display for RpcBlueWorkType {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{0:x}", self.0))
    }
}

impl From<BlueWorkType> for RpcBlueWorkType {
    fn from(item: BlueWorkType) -> RpcBlueWorkType {
        RpcBlueWorkType(item)
    }
}

impl From<RpcBlueWorkType> for BlueWorkType {
    fn from(item: RpcBlueWorkType) -> BlueWorkType {
        item.0
    }
}

impl From<RpcBlueWorkType> for String {
    fn from(item: RpcBlueWorkType) -> String {
        item.to_string()
    }
}

impl FromStr for RpcBlueWorkType {
    type Err = RpcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // TODO: replace the whole impl with math::Uint192::from_hex when available
        const HEX_LEN: usize = math::Uint192::BYTES * 2;
        let mut hex = [0u8; HEX_LEN];
        if s.len() > HEX_LEN {
            return Err(RpcError::HexParsingError(faster_hex::Error::InvalidLength(s.len())));
        }
        hex.iter_mut().rev().zip(s.bytes().rev()).for_each(|(target, source)| *target = source);
        if s.len() < HEX_LEN {
            hex[0..HEX_LEN - s.len()].iter_mut().for_each(|x| *x = b'0');
        }
        let mut bytes = [0u8; math::Uint192::BYTES];
        faster_hex::hex_decode(&hex, &mut bytes)?;
        Ok(RpcBlueWorkType(BlueWorkType::from_be_bytes(bytes)))
    }
}

impl TryFrom<&str> for RpcBlueWorkType {
    type Error = RpcError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for RpcBlueWorkType {
    type Error = RpcError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_blue_work() {
        const HEX_STR: &str = "1234567890abcdef1234567890abc";
        const HEX_BIN: u128 = 0x1234567890abcdef1234567890abc;

        let bw = BlueWorkType::from_u128(123456789012345678901234567890123456789);
        let rbw: RpcBlueWorkType = bw.into();
        assert_eq!(bw, BlueWorkType::from(rbw));

        let b1 = RpcBlueWorkType::try_from(HEX_STR).unwrap();
        assert_eq!(b1.to_string(), HEX_STR);
        assert_eq!(b1, RpcBlueWorkType(math::Uint192::from_u128(HEX_BIN)));

        let rbw2 = rbw;
        assert_eq!(rbw, rbw2);
    }

    #[test]
    fn test_rpc_blue_work_try_from() {
        const HEX_STR: &str = "d529a12";
        let b2 = BlueWorkType::from_u128(u128::from_str_radix(HEX_STR, 16).unwrap());

        assert!(RpcBlueWorkType::try_from(HEX_STR).is_ok());
        assert!(RpcBlueWorkType::try_from("not a number").is_err());

        assert_eq!(RpcBlueWorkType(b2), RpcBlueWorkType::from_str(HEX_STR).unwrap());
    }

    #[test]
    fn test_rpc_blue_work_from_str() {
        const HEX_STR: &str = "40a593f53f695ba413";
        let b2 = BlueWorkType::from_u128(u128::from_str_radix(HEX_STR, 16).unwrap());

        assert!(RpcBlueWorkType::from_str(HEX_STR).is_ok());
        assert!(RpcBlueWorkType::from_str("not a number").is_err());

        assert_eq!(RpcBlueWorkType(b2), RpcBlueWorkType::from_str(HEX_STR).unwrap());
    }
}
