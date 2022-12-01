extern crate derive_more;

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt;
use std::str::{self, FromStr};

use crate::errors;

// Represents binary data stringifyed in hexadecimal form
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase", try_from = "String", into = "String")]
pub struct RpcHexData(Vec<u8>);

impl fmt::Display for RpcHexData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // an empty vector is allowed
        if self.0.is_empty() {
            return f.write_str("");
        }

        let mut hex = vec![0u8; self.0.len() * 2];
        faster_hex::hex_encode(&self.0, hex.as_mut_slice()).expect("The output is exactly twice the size of the input");
        f.write_str(str::from_utf8(&hex).expect("hex is always valid UTF-8"))
    }
}

impl From<&[u8]> for RpcHexData {
    fn from(item: &[u8]) -> Self {
        RpcHexData(item.into())
    }
}

impl From<&Vec<u8>> for RpcHexData {
    fn from(item: &Vec<u8>) -> RpcHexData {
        RpcHexData(item.clone())
    }
}

impl From<RpcHexData> for String {
    fn from(item: RpcHexData) -> String {
        item.to_string()
    }
}

impl FromStr for RpcHexData {
    type Err = errors::RpcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // an empty string is allowed
        if s.is_empty() {
            return Ok(RpcHexData(vec![]));
        }

        let mut bytes = vec![0u8; s.len() / 2];
        faster_hex::hex_decode(s.as_bytes(), bytes.as_mut_slice())?;
        Ok(RpcHexData(bytes))
    }
}

impl TryFrom<&str> for RpcHexData {
    type Error = errors::RpcError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for RpcHexData {
    type Error = errors::RpcError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl AsRef<Vec<u8>> for RpcHexData {
    fn as_ref(&self) -> &Vec<u8> {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_script_key() {
        let raw: Vec<u8> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let skey: RpcHexData = RpcHexData::from(&raw);
        assert_eq!(raw, *skey.as_ref());
        assert_eq!(RpcHexData::from(&vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]), skey);

        assert!(RpcHexData::from_str("123456789012345678901234567890123456789").is_err());
        assert!(RpcHexData::from_str("1234567890123456789012345678901234567890").is_ok());
        assert!(RpcHexData::from_str("not a number").is_err());

        assert!(RpcHexData::try_from("123456789012345678901234567890123456789".to_string()).is_err());
        assert!(RpcHexData::try_from("1234567890123456789012345678901234567890".to_string()).is_ok());
        assert!(RpcHexData::try_from("not a number".to_string()).is_err());

        assert!(RpcHexData::try_from("10").is_ok());
        assert!(RpcHexData::try_from("aaFF").is_ok());
        assert!(RpcHexData::try_from("not a number").is_err());

        let skey2 = skey.clone();
        assert_eq!(skey, skey2);

        let code = "fedcba9876543210";
        let key = RpcHexData::try_from(code).unwrap();
        assert_eq!(key.to_string().to_lowercase(), code);
        assert_eq!(key.as_ref().len(), code.len() / 2);
    }
}
