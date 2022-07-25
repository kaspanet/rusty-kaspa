use hex;
use std::convert::TryInto;
use std::fmt::Debug;
use std::mem::size_of;
use std::str::FromStr;

const HASH_SIZE: usize = 32;

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, Default)]
pub struct Hash([u8; HASH_SIZE]);

impl ToString for Hash {
    fn to_string(&self) -> String {
        hex::encode(self.0)
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

impl Hash {
    pub fn new(bytes: &[u8]) -> Self {
        Self(<[u8; HASH_SIZE]>::try_from(<&[u8]>::clone(&bytes)).expect("Slice must have the length of Hash"))
    }

    pub fn new_unique() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);

        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self::from_u64(c)
    }

    pub fn from_u64(word: u64) -> Self {
        let mut bytes = [0u8; HASH_SIZE];
        bytes[0..size_of::<u64>()].copy_from_slice(&word.to_le_bytes());
        Hash(bytes)
    }

    const DEFAULT: Hash = Hash([0u8; 32]);

    pub fn is_default(&self) -> bool {
        self.eq(&Self::DEFAULT)
    }

    #[allow(dead_code)]
    pub fn from_str_slow(hash_str: &str) -> Result<Self, hex::FromHexError> {
        match hex::decode(hash_str)?.try_into() {
            Ok(bytes) => Ok(Hash(bytes)),
            Err(_) => Err(hex::FromHexError::InvalidStringLength),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_basics() {
        let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
        let hash = Hash::from_str(hash_str).unwrap();
        assert_eq!(hash_str, hash.to_string());
        assert!(!hash.is_default());

        assert_ne!(Hash::new_unique(), Hash::new_unique());

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

#[cfg(all(test, feature = "bench"))]
mod benches {
    extern crate test;
    use self::test::{black_box, Bencher};
    use super::*;

    #[bench]
    pub fn bench_from_str_slow(bh: &mut Bencher) {
        bh.iter(|| {
            for _ in 0..1000 {
                let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
                black_box(Hash::from_str_slow(hash_str).unwrap());
            }
        });
    }

    #[bench]
    pub fn bench_from_str_fast(bh: &mut Bencher) {
        bh.iter(|| {
            for _ in 0..1000 {
                let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
                black_box(Hash::from_str(hash_str).unwrap());
            }
        });
    }
}
