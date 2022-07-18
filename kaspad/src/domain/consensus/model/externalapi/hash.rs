use hex;
use std::convert::TryInto;
use std::fmt::{Display, Formatter};

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
    pub fn from_string(hash_str: &String) -> Self {
        let byte_array: [u8; DOMAIN_HASH_SIZE] = hex::decode(hash_str).unwrap().try_into().unwrap();
        DomainHash { byte_array }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_basics() {
        let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
        let hash = DomainHash::from_string(&hash_str.to_owned());
        assert_eq!(hash_str, hash.to_string());

        let hash2 = DomainHash::from_string(&hash_str.to_owned());
        assert_eq!(hash, hash2);

        let hash3 = DomainHash::from_string(
            &"8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3ab".to_owned(),
        );
        assert_ne!(hash2, hash3);
    }
}
