mod hashers;
mod pow_hashers;

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_utils::{
    hex::{FromHex, ToHex},
    mem_size::MemSizeEstimator,
    serde_impl_deser_fixed_bytes_ref, serde_impl_ser_fixed_bytes_ref,
};
use std::{
    array::TryFromSliceError,
    fmt::{Debug, Display, Formatter},
    hash::{Hash as StdHash, Hasher as StdHasher},
    str::{self, FromStr},
};
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

pub const HASH_SIZE: usize = 32;

pub use hashers::*;

// TODO: Check if we use hash more as an array of u64 or of bytes and change the default accordingly
/// @category General
#[derive(Eq, Clone, Copy, Default, PartialOrd, Ord, BorshSerialize, BorshDeserialize, CastFromJs)]
#[wasm_bindgen]
pub struct Hash([u8; HASH_SIZE]);

serde_impl_ser_fixed_bytes_ref!(Hash, HASH_SIZE);
serde_impl_deser_fixed_bytes_ref!(Hash, HASH_SIZE);

impl From<[u8; HASH_SIZE]> for Hash {
    fn from(value: [u8; HASH_SIZE]) -> Self {
        Hash(value)
    }
}

impl TryFrom<&[u8]> for Hash {
    type Error = TryFromSliceError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Hash::try_from_slice(value)
    }
}

impl Hash {
    #[inline(always)]
    pub const fn from_bytes(bytes: [u8; HASH_SIZE]) -> Self {
        Hash(bytes)
    }

    #[inline(always)]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }

    #[inline(always)]
    /// # Panics
    /// Panics if `bytes` length is not exactly `HASH_SIZE`.
    pub fn from_slice(bytes: &[u8]) -> Self {
        Self(<[u8; HASH_SIZE]>::try_from(bytes).expect("Slice must have the length of Hash"))
    }

    #[inline(always)]
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, TryFromSliceError> {
        Ok(Self(<[u8; HASH_SIZE]>::try_from(bytes)?))
    }

    #[inline(always)]
    pub fn to_le_u64(self) -> [u64; 4] {
        let mut out = [0u64; 4];
        out.iter_mut().zip(self.iter_le_u64()).for_each(|(out, word)| *out = word);
        out
    }

    #[inline(always)]
    pub fn iter_le_u64(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
        self.0.chunks_exact(8).map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
    }

    #[inline(always)]
    pub fn from_le_u64(arr: [u64; 4]) -> Self {
        let mut ret = [0; HASH_SIZE];
        ret.chunks_exact_mut(8).zip(arr.iter()).for_each(|(bytes, word)| bytes.copy_from_slice(&word.to_le_bytes()));
        Self(ret)
    }

    #[inline(always)]
    pub fn from_u64_word(word: u64) -> Self {
        Self::from_le_u64([0, 0, 0, word])
    }
}

// Override the default Hash implementation, to: A. improve perf a bit (siphash works over u64s), B. allow a hasher to just take the first u64.
// Don't change this without looking at `consensus/core/src/blockhash/BlockHashMap`.
impl StdHash for Hash {
    #[inline(always)]
    fn hash<H: StdHasher>(&self, state: &mut H) {
        self.iter_le_u64().for_each(|x| x.hash(state));
    }
}

/// We only override PartialEq because clippy wants us to.
/// This should always hold: PartialEq(x,y) => Hash(x) == Hash(y)
impl PartialEq for Hash {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Display for Hash {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut hex = [0u8; HASH_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("The output is exactly twice the size of the input");
        f.write_str(unsafe { str::from_utf8_unchecked(&hex) })
    }
}

impl Debug for Hash {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self, f)
    }
}

impl FromStr for Hash {
    type Err = faster_hex::Error;

    #[inline]
    fn from_str(hash_str: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0u8; HASH_SIZE];
        faster_hex::hex_decode(hash_str.as_bytes(), &mut bytes)?;
        Ok(Hash(bytes))
    }
}

impl From<u64> for Hash {
    #[inline(always)]
    fn from(word: u64) -> Self {
        Self::from_u64_word(word)
    }
}

impl AsRef<[u8; HASH_SIZE]> for Hash {
    #[inline(always)]
    fn as_ref(&self) -> &[u8; HASH_SIZE] {
        &self.0
    }
}

impl AsRef<[u8]> for Hash {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl ToHex for Hash {
    fn to_hex(&self) -> String {
        self.to_string()
    }
}

impl FromHex for Hash {
    type Error = faster_hex::Error;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        Self::from_str(hex_str)
    }
}

impl MemSizeEstimator for Hash {}

#[wasm_bindgen]
impl Hash {
    #[wasm_bindgen(constructor)]
    pub fn constructor(hex_str: &str) -> Self {
        Hash::from_str(hex_str).expect("invalid hash value")
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn js_to_string(&self) -> String {
        self.to_string()
    }
}

type TryFromError = workflow_wasm::error::Error;
impl TryCastFromJs for Hash {
    type Error = TryFromError;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            let bytes = value.as_ref().try_as_vec_u8()?;
            Ok(Hash(
                <[u8; HASH_SIZE]>::try_from(bytes)
                    .map_err(|_| TryFromError::WrongSize("Slice must have the length of Hash".into()))?,
            ))
        })
    }
}

pub const ZERO_HASH: Hash = Hash([0; HASH_SIZE]);

#[cfg(test)]
mod tests {
    use super::Hash;
    use std::str::FromStr;

    #[test]
    fn test_hash_basics() {
        let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
        let hash = Hash::from_str(hash_str).unwrap();
        assert_eq!(hash_str, hash.to_string());
        let hash2 = Hash::from_str(hash_str).unwrap();
        assert_eq!(hash, hash2);

        let hash3 = Hash::from_str("8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3ab").unwrap();
        assert_ne!(hash2, hash3);

        let odd_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3a";
        let short_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3";

        assert!(matches!(dbg!(Hash::from_str(odd_str)), Err(faster_hex::Error::InvalidLength(len)) if len == 64));
        assert!(matches!(dbg!(Hash::from_str(short_str)), Err(faster_hex::Error::InvalidLength(len)) if len == 64));
    }
}
