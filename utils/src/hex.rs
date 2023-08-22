use serde::{Deserialize, Deserializer, Serializer};
use smallvec::{smallvec, SmallVec};
use std::fmt::Debug;
use std::str;

pub trait ToHex {
    fn to_hex(&self) -> String;
}

pub fn serialize<S, T>(this: T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: ToHex,
{
    let hex = this.to_hex();
    serializer.serialize_str(&hex)
}

pub trait FromHex: Sized {
    type Error: std::fmt::Display;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error>;
}

pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromHex,
{
    use serde::de::Error;
    let buff: &[u8] = Deserialize::deserialize(deserializer)?;
    T::from_hex(str::from_utf8(buff).unwrap()).map_err(D::Error::custom)
}

/// Little endian format of full slice content
/// (so string lengths are always even).
impl ToHex for &[u8] {
    fn to_hex(&self) -> String {
        // an empty vector is allowed
        if self.is_empty() {
            return "".to_string();
        }

        let mut hex = vec![0u8; self.len() * 2];
        faster_hex::hex_encode(self, hex.as_mut_slice()).expect("The output is exactly twice the size of the input");
        let result = unsafe { str::from_utf8_unchecked(&hex) };
        result.to_string()
    }
}

/// Little endian format of full content
/// (so string lengths are always even).
impl ToHex for Vec<u8> {
    fn to_hex(&self) -> String {
        (&**self).to_hex()
    }
}

/// Little endian format of full content
/// (so string lengths must be even).
impl FromHex for Vec<u8> {
    type Error = faster_hex::Error;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        // an empty string is allowed
        if hex_str.is_empty() {
            return Ok(vec![]);
        }

        let mut bytes = vec![0u8; hex_str.len() / 2];
        faster_hex::hex_decode(hex_str.as_bytes(), bytes.as_mut_slice())?;
        Ok(bytes)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FixedArrayError<const N: usize> {
    #[error(transparent)]
    Deserialize(#[from] faster_hex::Error),
    #[error("unexpected length of hex string. actual: {0}")]
    WrongLength(usize),
}

/// Little endian format of full content
/// (so string lengths must be even).
impl<const N: usize> FromHex for [u8; N] {
    type Error = FixedArrayError<N>;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        let len = hex_str.len();
        if len != N * 2 {
            return Err(Self::Error::WrongLength(len));
        }

        let mut bytes = [0u8; N];
        faster_hex::hex_decode(hex_str.as_bytes(), bytes.as_mut_slice())?;
        Ok(bytes)
    }
}

/// Little endian format of full content
/// (so string lengths are always even).
impl<A: smallvec::Array<Item = u8>> ToHex for SmallVec<A> {
    fn to_hex(&self) -> String {
        (&**self).to_hex()
    }
}

/// Little endian format of full content
/// (so string lengths must be even).
impl<A: smallvec::Array<Item = u8>> FromHex for SmallVec<A> {
    type Error = faster_hex::Error;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        // an empty string is allowed
        if hex_str.is_empty() {
            return Ok(smallvec![]);
        }

        let mut bytes: SmallVec<A> = smallvec![0u8; hex_str.len() / 2];
        faster_hex::hex_decode(hex_str.as_bytes(), bytes.as_mut_slice())?;
        Ok(bytes)
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
        assert_eq!(k.to_string(), v.to_hex());
        assert_eq!(Vec::from_hex(k).unwrap(), v);

        assert!(Vec::from_hex("not a number").is_err());
        assert!(Vec::from_hex("ab01").is_ok());

        // even str length is required
        assert!(Vec::from_hex("ab0").is_err());
        // empty str is supported
        assert_eq!(Vec::from_hex("").unwrap().len(), 0);
    }

    #[test]
    fn test_smallvec_hex_convert() {
        type TestVec = SmallVec<[u8; 36]>;

        let v: TestVec = smallvec![0x0, 0xab, 0x55, 0x30, 0x1f, 0x63];
        let k = "00ab55301f63";
        assert_eq!(k.len(), v.len() * 2);
        assert_eq!(k.to_string(), v.to_hex());
        assert_eq!(SmallVec::<[u8; 36]>::from_hex(k).unwrap(), v);

        assert!(TestVec::from_hex("not a number").is_err());
        assert!(TestVec::from_hex("ab01").is_ok());

        // even str length is required
        assert!(TestVec::from_hex("ab0").is_err());
        // empty str is supported
        assert_eq!(TestVec::from_hex("").unwrap().len(), 0);
    }
}
