mod hashers;
mod pow_hashers;

use std::fmt::{Debug, Display, Formatter};
use std::str::{self, FromStr};

const HASH_SIZE: usize = 32;

pub use hashers::*;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Default, Debug)]
pub struct Hash([u8; HASH_SIZE]);

impl Hash {
    pub fn iter_u64_le(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
        self.0
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
    }

    fn from_u64_le(arr: [u64; 4]) -> Self {
        let mut ret = [0; HASH_SIZE];
        ret.chunks_exact_mut(8)
            .zip(arr.iter())
            .for_each(|(bytes, word)| bytes.copy_from_slice(&word.to_le_bytes()));
        Self(ret)
    }
}

impl Display for Hash {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut hex = [0u8; HASH_SIZE * 2];
        hex::encode_to_slice(&self.0, &mut hex).expect("The output is exactly twice the size of the input");
        f.write_str(str::from_utf8(&hex).expect("hex is always valid UTF-8"))
    }
}

impl FromStr for Hash {
    type Err = hex::FromHexError;

    fn from_str(hash_str: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0u8; HASH_SIZE];
        hex::decode_to_slice(hash_str, &mut bytes)?;
        Ok(Hash(bytes))
    }
}

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

        assert_eq!(Hash::from_str(odd_str), Err(hex::FromHexError::OddLength));
        assert_eq!(Hash::from_str(short_str), Err(hex::FromHexError::InvalidStringLength));
    }
}
