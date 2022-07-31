mod u3072;

use crate::u3072::U3072;
use hashes::{Hash, Hasher, MuHashElementHash, MuHashFinalizeHash};
use rand_chacha::rand_core::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use std::error::Error;
use std::fmt::Display;

pub const HASH_SIZE: usize = 32;
pub const SERIALIZED_MUHASH_SIZE: usize = ELEMENT_BYTE_SIZE;
// The hash of `NewMuHash().Finalize()`
pub const EMPTY_MUHASH: Hash = Hash::from_bytes([
    0x54, 0x4e, 0xb3, 0x14, 0x2c, 0x0, 0xf, 0xa, 0xd2, 0xc7, 0x6a, 0xc4, 0x1f, 0x42, 0x22, 0xab, 0xba, 0xba, 0xbe,
    0xd8, 0x30, 0xee, 0xaf, 0xee, 0x4b, 0x6d, 0xc5, 0x6b, 0x52, 0xd5, 0xca, 0xc0,
]);

pub(crate) const ELEMENT_BIT_SIZE: usize = 3072;
pub(crate) const ELEMENT_BYTE_SIZE: usize = ELEMENT_BIT_SIZE / 8;

/// MuHash is a type used to create a Multiplicative Hash
/// which is a rolling(homomorphic) hash that you can add and remove elements from
/// and receive the same resulting hash as-if you never hashed them.
/// Because of that the order of adding and removing elements doesn't matter.
#[derive(Clone, Debug)]
pub struct MuHash {
    numerator: U3072,
    denominator: U3072,
}

#[derive(Debug)]
pub struct OverflowError;

impl Display for OverflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Overflow in the MuHash field")
    }
}

impl Error for OverflowError {}

impl MuHash {
    /// return an empty initialized set.
    /// when finalized it should be equal to a finalized set with all elements removed.
    pub fn new() -> Self {
        Self { numerator: U3072::one(), denominator: U3072::one() }
    }

    // hashes the data and adds it to the muhash.
    // Supports arbitrary length data (subject to the underlying hash function(Blake2b) limits)
    pub fn add_element(&mut self, data: &[u8]) {
        let element = data_to_element(data);
        self.numerator *= element;
    }

    // hashes the data and removes it from the muhash.
    // Supports arbitrary length data (subject to the underlying hash function(Blake2b) limits)
    pub fn remove_element(&mut self, data: &[u8]) {
        let element = data_to_element(data);
        self.denominator *= element;
    }

    // will add the MuHash together. Equivalent to manually adding all the data elements
    // from one set to the other.
    pub fn combine(&mut self, other: &Self) {
        self.numerator *= other.denominator;
        self.denominator *= other.numerator;
    }

    pub fn finalize(mut self) -> Hash {
        let serialized = self.serialize();
        MuHashFinalizeHash::hash(serialized)
    }

    fn normalize(&mut self) {
        self.numerator /= self.denominator;
        self.denominator = U3072::one();
    }

    pub fn serialize(&mut self) -> [u8; SERIALIZED_MUHASH_SIZE] {
        self.normalize();
        self.numerator.to_le_bytes()
    }

    pub fn deserialize(data: [u8; SERIALIZED_MUHASH_SIZE]) -> Result<Self, OverflowError> {
        let numerator = U3072::from_le_bytes(data);
        if numerator.is_overflow() {
            Err(OverflowError)
        } else {
            Ok(Self { numerator, denominator: U3072::one() })
        }
    }
}

fn data_to_element(data: &[u8]) -> U3072 {
    let hash = MuHashElementHash::hash(data);
    let mut stream = ChaCha20Rng::from_seed(hash.as_bytes());
    let mut bytes = [0u8; ELEMENT_BYTE_SIZE];
    stream.fill_bytes(&mut bytes);
    U3072::from_le_bytes(bytes)
}

impl Default for MuHash {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::u3072;
    use crate::{MuHash, EMPTY_MUHASH, U3072};
    use hashes::Hash;
    use rand_chacha::rand_core::{RngCore, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    struct TestVector {
        data: &'static [u8],
        multiset_hash: Hash,
        cumulative_hash: Hash,
    }

    const TEST_VECTORS: [TestVector; 3] = [
        TestVector {
            data: &[
                152, 32, 81, 253, 30, 75, 167, 68, 187, 190, 104, 14, 31, 238, 20, 103, 123, 161, 163, 195, 84, 11,
                247, 177, 205, 182, 6, 232, 87, 35, 62, 14, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 242, 5, 42, 1, 0, 0, 0, 67,
                65, 4, 150, 181, 56, 232, 83, 81, 156, 114, 106, 44, 145, 230, 30, 193, 22, 0, 174, 19, 144, 129, 58,
                98, 124, 102, 251, 139, 231, 148, 123, 230, 60, 82, 218, 117, 137, 55, 149, 21, 212, 224, 166, 4, 248,
                20, 23, 129, 230, 34, 148, 114, 17, 102, 191, 98, 30, 115, 168, 44, 191, 35, 66, 200, 88, 238, 172,
            ],
            multiset_hash: Hash::from_bytes([
                44, 55, 150, 32, 253, 244, 236, 10, 194, 83, 203, 228, 186, 130, 194, 187, 220, 15, 237, 172, 127, 224,
                228, 82, 149, 125, 147, 117, 123, 191, 245, 193,
            ]),
            cumulative_hash: Hash::from_bytes([
                44, 55, 150, 32, 253, 244, 236, 10, 194, 83, 203, 228, 186, 130, 194, 187, 220, 15, 237, 172, 127, 224,
                228, 82, 149, 125, 147, 117, 123, 191, 245, 193,
            ]),
        },
        TestVector {
            data: &[
                213, 253, 204, 84, 30, 37, 222, 28, 122, 90, 221, 237, 242, 72, 88, 184, 187, 102, 92, 159, 54, 239,
                116, 78, 228, 44, 49, 96, 34, 201, 15, 155, 0, 0, 0, 0, 2, 0, 0, 0, 1, 0, 242, 5, 42, 1, 0, 0, 0, 67,
                65, 4, 114, 17, 168, 36, 245, 91, 80, 82, 40, 228, 195, 213, 25, 76, 31, 207, 170, 21, 164, 86, 171,
                223, 55, 249, 185, 217, 122, 64, 64, 175, 192, 115, 222, 230, 200, 144, 100, 152, 79, 3, 56, 82, 55,
                217, 33, 103, 193, 62, 35, 100, 70, 180, 23, 171, 121, 160, 252, 174, 65, 42, 227, 49, 107, 119, 172,
            ],
            multiset_hash: Hash::from_bytes([
                102, 139, 178, 146, 239, 21, 44, 84, 219, 15, 87, 20, 191, 69, 255, 141, 167, 177, 212, 28, 12, 80, 38,
                173, 101, 91, 47, 158, 27, 230, 126, 33,
            ]),
            cumulative_hash: Hash::from_bytes([
                177, 91, 209, 18, 74, 107, 82, 230, 78, 218, 60, 48, 35, 197, 135, 228, 85, 167, 158, 116, 140, 140,
                149, 77, 215, 65, 29, 13, 189, 151, 56, 99,
            ]),
        },
        TestVector {
            data: &[
                68, 246, 114, 34, 96, 144, 216, 93, 185, 169, 242, 251, 254, 95, 15, 150, 9, 179, 135, 175, 123, 229,
                183, 251, 183, 161, 118, 124, 131, 28, 158, 153, 0, 0, 0, 0, 3, 0, 0, 0, 1, 0, 242, 5, 42, 1, 0, 0, 0,
                67, 65, 4, 148, 185, 211, 231, 108, 91, 22, 41, 236, 249, 127, 255, 149, 215, 164, 187, 218, 200, 124,
                194, 96, 153, 173, 162, 128, 102, 198, 255, 30, 185, 25, 18, 35, 205, 137, 113, 148, 160, 141, 12, 39,
                38, 197, 116, 127, 29, 180, 158, 140, 249, 14, 117, 220, 62, 53, 80, 174, 155, 48, 8, 111, 60, 213,
                170, 172,
            ],
            multiset_hash: Hash::from_bytes([
                244, 11, 32, 189, 196, 62, 242, 240, 26, 23, 59, 118, 124, 185, 198, 184, 219, 86, 2, 235, 83, 95, 203,
                152, 39, 56, 95, 155, 14, 58, 250, 244,
            ]),
            cumulative_hash: Hash::from_bytes([
                230, 156, 110, 5, 4, 16, 118, 22, 72, 206, 98, 118, 168, 28, 128, 68, 185, 239, 177, 113, 94, 166, 246,
                251, 159, 140, 247, 168, 193, 232, 3, 150,
            ]),
        },
    ];

    const MAX_MU_HASH: MuHash = MuHash { numerator: U3072::MAX, denominator: U3072::MAX };

    #[test]
    fn test_random_muhash_arithmetic() {
        let element_from_byte = |b| [b; 32];
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
        let rng_get_byte = |rng: &mut ChaCha8Rng| {
            let mut byte = [0u8; 1];
            rng.fill_bytes(&mut byte);
            byte[0]
        };
        for _ in 0..10 {
            let mut res = Hash::default();
            let mut table = [0u8; 4];
            rng.fill_bytes(&mut table);

            for order in 0..4 {
                let mut acc = MuHash::new();
                for i in 0..4 {
                    let t = table[i ^ order];
                    if (t & 4) == 1 {
                        acc.remove_element(&element_from_byte(t & 3));
                    } else {
                        acc.add_element(&element_from_byte(t & 3));
                    }
                }
                let out = acc.finalize();
                if order == 0 {
                    res = out;
                } else {
                    assert_eq!(res, out);
                }
            }
            let mut x = element_from_byte(rng_get_byte(&mut rng)); // x=X
            let mut y = element_from_byte(rng_get_byte(&mut rng)); // x=X, y=Y
            let mut z = MuHash::new(); // x=X, y=X, z=1
            let mut yx = MuHash::new(); // x=X, y=Y, z=1 yx=1
            yx.add_element(&y); // x=X, y=X, z=1, yx=Y
            yx.add_element(&x); // x=X, y=X, z=1, yx=Y*X
            yx.normalize();
            z.add_element(&x); // x=X, y=Y, z=X, yx=Y*X
            z.add_element(&y); // x=X, y=Y, z=X*Y, yx = Y*X
            z.denominator *= yx.numerator; // x=X, y=Y, z=1, yx=Y*X

            let empty = MuHash::new();
            assert_eq!(EMPTY_MUHASH, empty.finalize());
            assert_eq!(z.finalize(), EMPTY_MUHASH);
        }
    }
}
