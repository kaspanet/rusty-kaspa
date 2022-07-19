use hex;
use std::convert::TryInto;
use std::fmt::Debug;

const DOMAIN_HASH_SIZE: usize = 32;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct DomainHash {
    byte_array: [u8; DOMAIN_HASH_SIZE],
}

impl ToString for DomainHash {
    fn to_string(&self) -> String {
        hex::encode(self.byte_array)
    }
}

// impl Display for DomainHash {
//     fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
//         write!(f, "{}", self.to_string())
//     }
// }

impl DomainHash {
    pub fn from_string(hash_str: &String) -> Result<Self, hex::FromHexError> {
        let mut byte_array = [0u8; DOMAIN_HASH_SIZE];
        hex::decode_to_slice(hash_str, &mut byte_array)?;
        Ok(DomainHash { byte_array })
    }

    pub fn from_string_slow(hash_str: &String) -> Result<Self, hex::FromHexError> {
        return match hex::decode(hash_str)?.try_into() {
            Ok(byte_array) => Ok(DomainHash { byte_array }),
            Err(_) => Err(hex::FromHexError::InvalidStringLength),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate test;
    use self::test::{black_box, Bencher};

    #[test]
    fn test_hash_basics() {
        let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
        let hash = DomainHash::from_string(&hash_str.to_owned()).unwrap();
        assert_eq!(hash_str, hash.to_string());

        let hash2 = DomainHash::from_string(&hash_str.to_owned()).unwrap();
        assert_eq!(hash, hash2);

        let hash3 = DomainHash::from_string(
            &"8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3ab".to_owned(),
        )
        .unwrap();
        assert_ne!(hash2, hash3);

        let odd_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3a";
        let short_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3";

        match DomainHash::from_string(&odd_str.to_owned()) {
            Ok(_) => panic!("Expected hex error"),
            Err(e) => match e {
                hex::FromHexError::OddLength => (),
                _ => panic!("Expected hex odd error"),
            },
        }

        match DomainHash::from_string(&short_str.to_owned()) {
            Ok(_) => panic!("Expected hex error"),
            Err(e) => match e {
                hex::FromHexError::InvalidStringLength => (),
                _ => panic!("Expected hex invalid length error"),
            },
        }
    }
}

// #[cfg(all(test, feature = "bench"))]
#[cfg(test)]
mod benches {
    extern crate test;
    use self::test::{black_box, Bencher};
    use super::*;

    #[bench]
    pub fn bench_from_string_slow(bh: &mut Bencher) {
        bh.iter(|| {
            for _ in 0..10000 {
                let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
                black_box(DomainHash::from_string_slow(&hash_str.to_owned()).unwrap());
            }
        });
    }

    #[bench]
    pub fn bench_from_string_fast(bh: &mut Bencher) {
        bh.iter(|| {
            for _ in 0..10000 {
                let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
                black_box(DomainHash::from_string(&hash_str.to_owned()).unwrap());
            }
        });
    }
}
