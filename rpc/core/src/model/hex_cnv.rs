use consensus_core::BlueWorkType;
use smallvec::{smallvec, SmallVec};
use std::str;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum HexConversionError {
    #[error("{0}")]
    HexParsingError(#[from] faster_hex::Error),

    #[error("not supported")]
    NotSupportedError,
}

pub trait HexConversion: Sized {
    fn to_hex_string(&self) -> String;
    fn from_hex_str(hex_str: &str) -> Result<Self, HexConversionError>;
}

/// Little endian format of full slice content
/// (so string lengths are always even).
impl HexConversion for &[u8] {
    fn to_hex_string(&self) -> String {
        // an empty vector is allowed
        if self.is_empty() {
            return "".to_string();
        }

        let mut hex = vec![0u8; self.len() * 2];
        faster_hex::hex_encode(self, hex.as_mut_slice()).expect("The output is exactly twice the size of the input");
        let result = str::from_utf8(&hex).expect("hex is always valid UTF-8");
        result.to_string()
    }

    fn from_hex_str(_: &str) -> Result<Self, HexConversionError> {
        Err(HexConversionError::NotSupportedError)
    }
}

/// Little endian format of full vec content
/// (so string lengths are always even).
impl HexConversion for Vec<u8> {
    fn to_hex_string(&self) -> String {
        // an empty vector is allowed
        if self.is_empty() {
            return "".to_string();
        }

        let mut hex = vec![0u8; self.len() * 2];
        faster_hex::hex_encode(self, hex.as_mut_slice()).expect("The output is exactly twice the size of the input");
        let result = str::from_utf8(&hex).expect("hex is always valid UTF-8");
        result.to_string()
    }

    fn from_hex_str(hex_str: &str) -> Result<Self, HexConversionError> {
        // an empty string is allowed
        if hex_str.is_empty() {
            return Ok(vec![]);
        }

        let mut bytes = vec![0u8; hex_str.len() / 2];
        faster_hex::hex_decode(hex_str.as_bytes(), bytes.as_mut_slice())?;
        Ok(bytes)
    }
}

/// Little endian format of full smallvec content
/// (so string lengths are always even).
impl<A: smallvec::Array<Item = u8>> HexConversion for SmallVec<A> {
    fn to_hex_string(&self) -> String {
        // an empty vector is allowed
        if self.is_empty() {
            return "".to_string();
        }

        // Note: defining the underlying array size to 96 is kind of a hack.
        // We take advantage of tha fact that SmallVec currently used here
        // have a size if 36.
        // Should this be implemented for a SmallVec of size greater then 48,
        // the hex buffer would be automatically spilled to the heap.
        let mut hex: SmallVec<[u8; 96]> = smallvec![0u8; self.len() * 2];

        faster_hex::hex_encode(self, hex.as_mut_slice()).expect("The output is exactly twice the size of the input");
        let result = str::from_utf8(&hex).expect("hex is always valid UTF-8");
        result.to_string()
    }

    fn from_hex_str(hex_str: &str) -> Result<Self, HexConversionError> {
        // an empty string is allowed
        if hex_str.is_empty() {
            return Ok(smallvec![]);
        }

        let mut bytes: SmallVec<A> = smallvec![0u8; hex_str.len() / 2];
        faster_hex::hex_decode(hex_str.as_bytes(), bytes.as_mut_slice())?;
        Ok(bytes)
    }
}

/// This implementation is of big endian format.
/// Leading '0' are ignored by str parsing and absent of string result.
/// Odd str lengths are valid.
impl HexConversion for BlueWorkType {
    fn to_hex_string(&self) -> String {
        format!("{0:x}", self)
    }

    fn from_hex_str(hex_str: &str) -> Result<Self, HexConversionError> {
        Ok(BlueWorkType::from_hex(hex_str)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec_hex_convert() {
        let v: Vec<u8> = vec![0x0, 0xab, 0x55, 0x30, 0x1f, 0x63];
        let k = "00ab55301f63";
        assert_eq!(k.len(), v.len() * 2);
        assert_eq!(k.to_string(), v.to_hex_string());
        assert_eq!(Vec::from_hex_str(k).unwrap(), v);

        assert!(Vec::from_hex_str("not a number").is_err());
        assert!(Vec::from_hex_str("ab01").is_ok());

        // even str length is required
        assert!(Vec::from_hex_str("ab0").is_err());
        // empty str is supported
        assert_eq!(Vec::from_hex_str("").unwrap().len(), 0);
    }

    #[test]
    fn test_smallvec_hex_convert() {
        type TestVec = SmallVec<[u8; 36]>;

        let v: TestVec = smallvec![0x0, 0xab, 0x55, 0x30, 0x1f, 0x63];
        let k = "00ab55301f63";
        assert_eq!(k.len(), v.len() * 2);
        assert_eq!(k.to_string(), v.to_hex_string());
        assert_eq!(SmallVec::<[u8; 36]>::from_hex_str(k).unwrap(), v);

        assert!(TestVec::from_hex_str("not a number").is_err());
        assert!(TestVec::from_hex_str("ab01").is_ok());

        // even str length is required
        assert!(TestVec::from_hex_str("ab0").is_err());
        // empty str is supported
        assert_eq!(TestVec::from_hex_str("").unwrap().len(), 0);
    }

    #[test]
    fn test_blue_work_type_hex_convert() {
        const HEX_STR: &str = "a1b21";
        const HEX_VAL: u64 = 0xa1b21;
        let b: BlueWorkType = BlueWorkType::from_u64(HEX_VAL);
        assert_eq!(HEX_STR.to_string(), b.to_hex_string());
        assert!(BlueWorkType::from_hex_str("not a number").is_err());

        // max str len is 48 for a 192 bits Uint
        // odd lengths are accepted
        // leading '0' are ignored
        // empty str is supported
        const TEST_STR: &str = "000fedcba987654321000000a9876543210fedcba9876543210fedcba9876543210";
        for i in 0..TEST_STR.len() {
            assert!(BlueWorkType::from_hex_str(&TEST_STR[0..i]).is_ok() == (i <= 48));
            if 0 < i && i < 33 {
                let b = BlueWorkType::from_hex_str(&TEST_STR[0..i]).unwrap();
                let u = u128::from_str_radix(&TEST_STR[0..i], 16).unwrap();
                assert_eq!(b, BlueWorkType::from_u128(u));
                assert_eq!(b.to_hex_string(), format!("{0:x}", u));
            }
        }
    }
}
