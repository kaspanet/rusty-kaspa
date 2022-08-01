use hex;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display, Formatter};
use std::mem::size_of;
use std::rc::Rc;
use std::str::FromStr;

const HASH_SIZE: usize = 32;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Default, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Hash([u8; HASH_SIZE]);

impl Display for Hash {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
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

impl From<u64> for Hash {
    fn from(word: u64) -> Self {
        Self::from_u64(word)
    }
}

impl Debug for Hash {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0[8..32].eq(&[0u8; 24]) {
            let word: u64 = u64::from_le_bytes(<[u8; 8]>::try_from(&self.0[0..8]).unwrap());
            f.debug_tuple("Hash").field(&word).finish()
        } else {
            f.debug_tuple("Hash").field(&self.0).finish()
        }
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Hash {
    pub fn new(bytes: &[u8]) -> Self {
        Self(<[u8; HASH_SIZE]>::try_from(bytes).expect("Slice must have the length of Hash"))
    }

    pub fn new_unique() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);

        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self::from_u64(c)
    }

    pub fn from_u64(word: u64) -> Self {
        assert_ne!(word, 0, "0 is reserved for the default Hash");
        let mut bytes = [0u8; HASH_SIZE];
        bytes[0..size_of::<u64>()].copy_from_slice(&word.to_le_bytes());
        Hash(bytes)
    }

    pub const ZERO: Hash = Hash([0u8; HASH_SIZE]);
    pub const VIRTUAL: Hash = Hash([0xff; HASH_SIZE]);
    pub const ORIGIN: Hash = Hash([0xfe; HASH_SIZE]);

    /// `Hash::ZERO` is the `null` block hash.
    pub fn is_zero(&self) -> bool {
        self.eq(&Self::ZERO)
    }

    /// `Hash::VIRTUAL` is a special hash representing the `virtual` block.
    pub fn is_virtual(&self) -> bool {
        self.eq(&Self::VIRTUAL)
    }

    /// `Hash::ORIGIN` is a special hash representing a `virtual genesis` block.
    /// It serves as a special local block which all locally-known
    /// blocks are in its future.
    pub fn is_origin(&self) -> bool {
        self.eq(&Self::ORIGIN)
    }
}

pub type HashArray = Rc<Vec<Hash>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_basics() {
        let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
        let hash = Hash::from_str(hash_str).unwrap();
        assert_eq!(hash_str, hash.to_string());
        assert!(!hash.is_zero());

        assert_ne!(Hash::new_unique(), Hash::new_unique());
        let vec = vec![2u8; 32];
        let _ = Hash::new(&vec);

        let hash2 = Hash::from_str(hash_str).unwrap();
        assert_eq!(hash, hash2);

        let hash3 = Hash::from_str("8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3ab").unwrap();
        assert_ne!(hash2, hash3);

        let odd_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3a";
        let short_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3";

        match Hash::from_str(odd_str) {
            Ok(_) => panic!("Expected hex error"),
            Err(e) => match e {
                hex::FromHexError::OddLength => (),
                _ => panic!("Expected hex odd error"),
            },
        }

        match Hash::from_str(short_str) {
            Ok(_) => panic!("Expected hex error"),
            Err(e) => match e {
                hex::FromHexError::InvalidStringLength => (),
                _ => panic!("Expected hex invalid length error"),
            },
        }
    }

    #[test]
    fn test_from_u64() {
        let _ = Hash::from_u64(7);
        // println!("{}", hash.to_string());
    }
}
