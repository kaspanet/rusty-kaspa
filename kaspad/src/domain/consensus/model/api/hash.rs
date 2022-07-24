use hex;
use std::convert::TryInto;
use std::fmt::Debug;
use std::mem::size_of;
use std::str::FromStr;

const DOMAIN_HASH_SIZE: usize = 32;

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, Default)]
pub struct DomainHash {
    byte_array: [u8; DOMAIN_HASH_SIZE],
}

impl ToString for DomainHash {
    fn to_string(&self) -> String {
        hex::encode(self.byte_array)
    }
}

impl FromStr for DomainHash {
    type Err = hex::FromHexError;

    fn from_str(hash_str: &str) -> Result<Self, Self::Err> {
        let mut byte_array = [0u8; DOMAIN_HASH_SIZE];
        hex::decode_to_slice(hash_str, &mut byte_array)?;
        Ok(DomainHash { byte_array })
    }
}

impl DomainHash {
    pub fn from_u64(word: u64) -> Self {
        let mut byte_array = [0u8; DOMAIN_HASH_SIZE];
        byte_array[0..size_of::<u64>()].copy_from_slice(&word.to_le_bytes());
        DomainHash { byte_array }
    }

    pub fn has_default_value(&self) -> bool {
        *self == Default::default()
    }

    #[allow(dead_code)]
    pub fn from_str_slow(hash_str: &str) -> Result<Self, hex::FromHexError> {
        match hex::decode(hash_str)?.try_into() {
            Ok(byte_array) => Ok(DomainHash { byte_array }),
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
        let hash = DomainHash::from_str(hash_str).unwrap();
        assert_eq!(hash_str, hash.to_string());
        assert!(!hash.has_default_value());

        let hash2 = DomainHash::from_str(hash_str).unwrap();
        assert_eq!(hash, hash2);

        let hash3 = DomainHash::from_str("8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3ab").unwrap();
        assert_ne!(hash2, hash3);

        let odd_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3a";
        let short_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3";

        match DomainHash::from_str(odd_str) {
            Ok(_) => panic!("Expected hex error"),
            Err(e) => match e {
                hex::FromHexError::OddLength => (),
                _ => panic!("Expected hex odd error"),
            },
        }

        match DomainHash::from_str(short_str) {
            Ok(_) => panic!("Expected hex error"),
            Err(e) => match e {
                hex::FromHexError::InvalidStringLength => (),
                _ => panic!("Expected hex invalid length error"),
            },
        }
    }

    #[test]
    fn test_from_u64() {
        let _ = DomainHash::from_u64(7);
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
                black_box(DomainHash::from_str_slow(hash_str).unwrap());
            }
        });
    }

    #[bench]
    pub fn bench_from_str_fast(bh: &mut Bencher) {
        bh.iter(|| {
            for _ in 0..1000 {
                let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
                black_box(DomainHash::from_str(hash_str).unwrap());
            }
        });
    }
}
