// Make u3072 public if we're fuzzing
#[cfg(fuzzing)]
pub mod u3072;
#[cfg(not(fuzzing))]
mod u3072;

use crate::u3072::U3072;
use kaspa_hashes::{Hash, Hasher, HasherBase, MuHashElementHash, MuHashFinalizeHash};
use kaspa_math::Uint3072;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::Display;

pub const HASH_SIZE: usize = 32;
pub const SERIALIZED_MUHASH_SIZE: usize = ELEMENT_BYTE_SIZE;
// The hash of `NewMuHash().Finalize()`
pub const EMPTY_MUHASH: Hash = Hash::from_bytes([
    0x54, 0x4e, 0xb3, 0x14, 0x2c, 0x0, 0xf, 0xa, 0xd2, 0xc7, 0x6a, 0xc4, 0x1f, 0x42, 0x22, 0xab, 0xba, 0xba, 0xbe, 0xd8, 0x30, 0xee,
    0xaf, 0xee, 0x4b, 0x6d, 0xc5, 0x6b, 0x52, 0xd5, 0xca, 0xc0,
]);

pub(crate) const ELEMENT_BIT_SIZE: usize = 3072;
pub(crate) const ELEMENT_BYTE_SIZE: usize = ELEMENT_BIT_SIZE / 8;

/// MuHash is a type used to create a Multiplicative Hash
/// which is a rolling(homomorphic) hash that you can add and remove elements from
/// and receive the same resulting hash as-if you never hashed them.
/// Because of that the order of adding and removing elements doesn't matter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MuHash {
    numerator: U3072,
    denominator: U3072,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OverflowError;

impl Display for OverflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Overflow in the MuHash field")
    }
}

impl Error for OverflowError {}

impl MuHash {
    #[inline]
    /// return an empty initialized set.
    /// when finalized it should be equal to a finalized set with all elements removed.
    pub fn new() -> Self {
        Self { numerator: U3072::one(), denominator: U3072::one() }
    }

    #[inline]
    // hashes the data and adds it to the muhash.
    // Supports arbitrary length data (subject to the underlying hash function(Blake2b) limits)
    pub fn add_element(&mut self, data: &[u8]) {
        let element = data_to_element(data);
        self.numerator *= element;
    }

    #[inline]
    // hashes the data and removes it from the muhash.
    // Supports arbitrary length data (subject to the underlying hash function(Blake2b) limits)
    pub fn remove_element(&mut self, data: &[u8]) {
        let element = data_to_element(data);
        self.denominator *= element;
    }

    #[inline]
    // returns a hasher for hashing data which on `finalize` adds the finalized hash to the muhash.
    pub fn add_element_builder(&mut self) -> MuHashElementBuilder<'_> {
        MuHashElementBuilder::new(&mut self.numerator)
    }

    #[inline]
    // returns a hasher for hashing data which on `finalize` removes the finalized hash from the muhash.
    pub fn remove_element_builder(&mut self) -> MuHashElementBuilder<'_> {
        MuHashElementBuilder::new(&mut self.denominator)
    }

    #[inline]
    // will add the MuHash together. Equivalent to manually adding all the data elements
    // from one set to the other.
    pub fn combine(&mut self, other: &Self) {
        self.numerator *= other.numerator;
        self.denominator *= other.denominator;
    }

    #[inline]
    pub fn finalize(&mut self) -> Hash {
        let serialized = self.serialize();
        MuHashFinalizeHash::hash(serialized)
    }

    #[inline]
    fn normalize(&mut self) {
        self.numerator /= self.denominator;
        self.denominator = U3072::one();
    }

    #[inline]
    pub fn serialize(&mut self) -> [u8; SERIALIZED_MUHASH_SIZE] {
        self.normalize();
        self.numerator.to_le_bytes()
    }

    #[inline]
    pub fn deserialize(data: [u8; SERIALIZED_MUHASH_SIZE]) -> Result<Self, OverflowError> {
        let numerator = U3072::from_le_bytes(data);
        if numerator.is_overflow() {
            Err(OverflowError)
        } else {
            Ok(Self { numerator, denominator: U3072::one() })
        }
    }
}

#[derive(Debug)]
pub enum MuHashError {
    NonNormalizedValue,
}

impl TryFrom<MuHash> for Uint3072 {
    type Error = MuHashError;

    fn try_from(value: MuHash) -> Result<Self, Self::Error> {
        if value.denominator == U3072::one() {
            Ok(value.numerator.into())
        } else {
            Err(MuHashError::NonNormalizedValue)
        }
    }
}

impl From<Uint3072> for MuHash {
    fn from(u: Uint3072) -> Self {
        MuHash { numerator: u.into(), denominator: U3072::one() }
    }
}

pub struct MuHashElementBuilder<'a> {
    muhash_field: &'a mut U3072,
    element_hasher: MuHashElementHash,
}

impl HasherBase for MuHashElementBuilder<'_> {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self {
        self.element_hasher.write(data);
        self
    }
}

impl<'a> MuHashElementBuilder<'a> {
    pub fn new(muhash_field: &'a mut U3072) -> Self {
        Self { muhash_field, element_hasher: MuHashElementHash::new() }
    }

    pub fn finalize(self) {
        let hash = self.element_hasher.finalize();
        let mut stream = ChaCha20Rng::from_seed(hash.as_bytes());
        let mut bytes = [0u8; ELEMENT_BYTE_SIZE];
        stream.fill_bytes(&mut bytes);
        *self.muhash_field *= U3072::from_le_bytes(bytes);
    }
}

#[inline]
fn data_to_element(data: &[u8]) -> U3072 {
    let hash = MuHashElementHash::hash(data);
    let mut stream = ChaCha20Rng::from_seed(hash.as_bytes());
    let mut bytes = [0u8; ELEMENT_BYTE_SIZE];
    stream.fill_bytes(&mut bytes);
    U3072::from_le_bytes(bytes)
}

impl Default for MuHash {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::OverflowError;
    use crate::{MuHash, EMPTY_MUHASH, U3072};
    use kaspa_hashes::Hash;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    struct TestVector {
        data: &'static [u8],
        multiset_hash: Hash,
        cumulative_hash: Hash,
    }

    const TEST_VECTORS: [TestVector; 3] = [
        TestVector {
            data: &[
                152, 32, 81, 253, 30, 75, 167, 68, 187, 190, 104, 14, 31, 238, 20, 103, 123, 161, 163, 195, 84, 11, 247, 177, 205,
                182, 6, 232, 87, 35, 62, 14, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 242, 5, 42, 1, 0, 0, 0, 67, 65, 4, 150, 181, 56, 232, 83,
                81, 156, 114, 106, 44, 145, 230, 30, 193, 22, 0, 174, 19, 144, 129, 58, 98, 124, 102, 251, 139, 231, 148, 123, 230,
                60, 82, 218, 117, 137, 55, 149, 21, 212, 224, 166, 4, 248, 20, 23, 129, 230, 34, 148, 114, 17, 102, 191, 98, 30, 115,
                168, 44, 191, 35, 66, 200, 88, 238, 172,
            ],
            multiset_hash: Hash::from_bytes([
                44, 55, 150, 32, 253, 244, 236, 10, 194, 83, 203, 228, 186, 130, 194, 187, 220, 15, 237, 172, 127, 224, 228, 82, 149,
                125, 147, 117, 123, 191, 245, 193,
            ]),
            cumulative_hash: Hash::from_bytes([
                44, 55, 150, 32, 253, 244, 236, 10, 194, 83, 203, 228, 186, 130, 194, 187, 220, 15, 237, 172, 127, 224, 228, 82, 149,
                125, 147, 117, 123, 191, 245, 193,
            ]),
        },
        TestVector {
            data: &[
                213, 253, 204, 84, 30, 37, 222, 28, 122, 90, 221, 237, 242, 72, 88, 184, 187, 102, 92, 159, 54, 239, 116, 78, 228, 44,
                49, 96, 34, 201, 15, 155, 0, 0, 0, 0, 2, 0, 0, 0, 1, 0, 242, 5, 42, 1, 0, 0, 0, 67, 65, 4, 114, 17, 168, 36, 245, 91,
                80, 82, 40, 228, 195, 213, 25, 76, 31, 207, 170, 21, 164, 86, 171, 223, 55, 249, 185, 217, 122, 64, 64, 175, 192, 115,
                222, 230, 200, 144, 100, 152, 79, 3, 56, 82, 55, 217, 33, 103, 193, 62, 35, 100, 70, 180, 23, 171, 121, 160, 252, 174,
                65, 42, 227, 49, 107, 119, 172,
            ],
            multiset_hash: Hash::from_bytes([
                102, 139, 178, 146, 239, 21, 44, 84, 219, 15, 87, 20, 191, 69, 255, 141, 167, 177, 212, 28, 12, 80, 38, 173, 101, 91,
                47, 158, 27, 230, 126, 33,
            ]),
            cumulative_hash: Hash::from_bytes([
                177, 91, 209, 18, 74, 107, 82, 230, 78, 218, 60, 48, 35, 197, 135, 228, 85, 167, 158, 116, 140, 140, 149, 77, 215, 65,
                29, 13, 189, 151, 56, 99,
            ]),
        },
        TestVector {
            data: &[
                68, 246, 114, 34, 96, 144, 216, 93, 185, 169, 242, 251, 254, 95, 15, 150, 9, 179, 135, 175, 123, 229, 183, 251, 183,
                161, 118, 124, 131, 28, 158, 153, 0, 0, 0, 0, 3, 0, 0, 0, 1, 0, 242, 5, 42, 1, 0, 0, 0, 67, 65, 4, 148, 185, 211, 231,
                108, 91, 22, 41, 236, 249, 127, 255, 149, 215, 164, 187, 218, 200, 124, 194, 96, 153, 173, 162, 128, 102, 198, 255,
                30, 185, 25, 18, 35, 205, 137, 113, 148, 160, 141, 12, 39, 38, 197, 116, 127, 29, 180, 158, 140, 249, 14, 117, 220,
                62, 53, 80, 174, 155, 48, 8, 111, 60, 213, 170, 172,
            ],
            multiset_hash: Hash::from_bytes([
                244, 11, 32, 189, 196, 62, 242, 240, 26, 23, 59, 118, 124, 185, 198, 184, 219, 86, 2, 235, 83, 95, 203, 152, 39, 56,
                95, 155, 14, 58, 250, 244,
            ]),
            cumulative_hash: Hash::from_bytes([
                230, 156, 110, 5, 4, 16, 118, 22, 72, 206, 98, 118, 168, 28, 128, 68, 185, 239, 177, 113, 94, 166, 246, 251, 159, 140,
                247, 168, 193, 232, 3, 150,
            ]),
        },
    ];

    fn element_from_byte(b: u8) -> [u8; 32] {
        let mut out = [0u8; 32];
        out[0] = b;
        out
    }

    #[test]
    fn test_random_muhash_arithmetic() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        for _ in 0..10 {
            let mut res = Hash::default();
            let mut table = [0u8; 4];
            rng.fill(&mut table[..]);

            for order in 0..4 {
                let mut acc = MuHash::new();
                for i in 0..4 {
                    let t = table[i ^ order];
                    if (t & 4) != 0 {
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
            let x = element_from_byte(rng.gen()); // x=X
            let y = element_from_byte(rng.gen()); // x=X, y=Y
            let mut z = MuHash::new(); // x=X, y=X, z=1
            let mut yx = MuHash::new(); // x=X, y=Y, z=1 yx=1
            yx.add_element(&y); // x=X, y=X, z=1, yx=Y
            yx.add_element(&x); // x=X, y=X, z=1, yx=Y*X
            yx.normalize();
            z.add_element(&x); // x=X, y=Y, z=X, yx=Y*X
            z.add_element(&y); // x=X, y=Y, z=X*Y, yx = Y*X
            z.denominator *= yx.numerator; // x=X, y=Y, z=1, yx=Y*X
            assert_eq!(z.finalize(), EMPTY_MUHASH);
        }
    }

    #[test]
    fn test_empty_hash() {
        let mut empty = MuHash::new();
        assert_eq!(empty.finalize(), EMPTY_MUHASH);
    }

    #[test]
    fn test_new_pre_computed() {
        let expected = "b557f7cfc13cf9abc31374832715e7bff2cf5859897523337a0ead9dde012974";
        let mut acc = MuHash::new();
        acc.add_element(&element_from_byte(0));
        acc.add_element(&element_from_byte(1));
        acc.remove_element(&element_from_byte(2));
        assert_eq!(acc.finalize().to_string(), expected);
    }

    #[test]
    fn test_serialize() {
        let expected = [
            50, 5, 73, 166, 198, 210, 31, 202, 37, 64, 219, 222, 57, 158, 121, 89, 67, 188, 211, 73, 217, 251, 250, 178, 135, 196, 39,
            250, 122, 202, 56, 228, 146, 233, 249, 16, 68, 9, 255, 158, 152, 84, 168, 146, 121, 81, 181, 60, 96, 141, 114, 26, 127,
            140, 164, 90, 87, 187, 24, 4, 187, 151, 135, 91, 9, 249, 103, 124, 91, 55, 72, 202, 43, 241, 196, 243, 201, 237, 141, 158,
            166, 125, 185, 26, 201, 232, 80, 72, 3, 7, 248, 152, 116, 148, 44, 250, 108, 167, 175, 61, 128, 159, 48, 148, 28, 247, 22,
            158, 40, 130, 41, 154, 93, 184, 199, 177, 0, 170, 212, 159, 61, 233, 131, 243, 16, 17, 246, 132, 114, 31, 155, 37, 25, 97,
            107, 11, 100, 17, 23, 61, 12, 218, 176, 129, 173, 148, 221, 6, 152, 157, 112, 106, 90, 5, 215, 0, 133, 133, 41, 241, 217,
            237, 6, 202, 106, 252, 196, 244, 209, 141, 220, 236, 40, 221, 219, 122, 222, 96, 27, 189, 60, 69, 150, 124, 29, 78, 206,
            249, 146, 179, 191, 11, 187, 178, 48, 114, 127, 155, 74, 137, 140, 109, 182, 88, 192, 120, 71, 141, 197, 93, 178, 179,
            254, 252, 167, 251, 245, 77, 112, 186, 216, 30, 239, 147, 168, 67, 89, 96, 14, 102, 165, 187, 163, 232, 51, 77, 117, 134,
            160, 254, 89, 201, 57, 113, 76, 137, 99, 101, 233, 35, 46, 213, 124, 38, 247, 12, 125, 203, 220, 54, 114, 68, 242, 192,
            107, 216, 226, 140, 66, 78, 65, 166, 255, 4, 2, 89, 247, 184, 204, 145, 54, 105, 210, 209, 195, 248, 63, 207, 199, 218,
            253, 92, 150, 190, 212, 216, 23, 121, 18, 14, 27, 35, 191, 203, 50, 238, 10, 190, 192, 47, 210, 100, 58, 38, 201, 103,
            199, 59, 32, 72, 37, 221, 104, 87, 120, 222, 61, 144, 107, 107, 114, 27, 152, 88, 232, 113, 97, 184, 69, 116, 17, 59, 245,
            151, 99, 140, 167, 85, 47, 28, 51, 198, 140, 233, 21, 92, 211, 79, 1, 68, 217, 131, 37, 19, 5, 107, 51, 219, 141, 109,
            155, 196, 183, 148, 16, 113, 227, 141, 202, 215, 191, 50, 241, 244,
        ];

        let mut check = MuHash::new();
        check.add_element(&element_from_byte(1));
        check.add_element(&element_from_byte(2));
        let ser = check.serialize();
        assert_eq!(ser, expected);

        let mut deserialized = MuHash::deserialize(ser).unwrap();
        assert_eq!(deserialized.finalize(), check.finalize());
        let overflow = [255; 384];
        assert_eq!(MuHash::deserialize(overflow).unwrap_err(), OverflowError);

        let mut zeroed = MuHash::new();
        zeroed.numerator *= U3072::zero();
        assert_eq!(zeroed.serialize(), [0u8; 384]);

        let mut deserialized = MuHash::deserialize(zeroed.serialize()).unwrap();
        zeroed.normalize();
        deserialized.normalize();
        assert_eq!(zeroed.numerator, deserialized.numerator);
    }

    #[test]
    fn test_vectors_hash() {
        for test in TEST_VECTORS {
            let mut m = MuHash::new();
            m.add_element(test.data);
            assert_eq!(m.finalize(), test.multiset_hash);
        }
    }
    #[test]
    fn test_vectors_add_remove() {
        let mut m = MuHash::new();

        for test in TEST_VECTORS {
            m.add_element(test.data);
            assert_eq!(m.finalize(), test.cumulative_hash);
        }

        for (i, test) in TEST_VECTORS.iter().enumerate().rev() {
            m.remove_element(test.data);
            if i != 0 {
                assert_eq!(m.finalize(), TEST_VECTORS[i - 1].cumulative_hash);
            }
        }
        assert_eq!(m.finalize(), EMPTY_MUHASH);
    }

    #[test]
    fn test_vectors_combine_subtract() {
        let mut m1 = MuHash::new();
        let mut m2 = MuHash::new();
        for test in TEST_VECTORS {
            m1.add_element(test.data);
            m2.remove_element(test.data);
        }
        let m1_orig = m1.clone();
        m1.combine(&m2);
        m2.combine(&m1_orig);
        assert_eq!(m1.finalize(), m2.finalize());
        assert_eq!(m1.finalize(), EMPTY_MUHASH);
    }

    #[test]
    fn test_vectors_commutativity() {
        // Here we first remove an element from an empty multiset, and then add some other
        // elements, and then we create a new empty multiset, then we add the same elements
        // we added to the previous multiset, and then we remove the same element we remove
        // the same element we removed from the previous multiset. According to commutativity
        // laws, the result should be the same.
        for (remove_index, _) in TEST_VECTORS.iter().enumerate() {
            let remove_data = TEST_VECTORS[remove_index].data;
            let mut m1 = MuHash::new();
            let mut m2 = MuHash::new();
            m1.remove_element(remove_data);
            for (i, test) in TEST_VECTORS.iter().enumerate() {
                if i != remove_index {
                    m1.add_element(test.data);
                    m2.add_element(test.data);
                }
            }
            m2.remove_element(remove_data);
            assert_eq!(m1.finalize(), m2.finalize());
        }
    }

    #[test]
    fn test_parse_muhash_fail() {
        let mut serialized = [255; 384];
        serialized[0..3].copy_from_slice(&[155, 40, 239]);

        assert_eq!(MuHash::deserialize(serialized).unwrap_err(), OverflowError);

        serialized[0] = 0;
        let _ = MuHash::deserialize(serialized).unwrap();
    }

    #[test]
    fn test_muhash_add_remove() {
        const LOOPS: usize = 1024;
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut set = MuHash::new();
        let list: Vec<_> = (0..LOOPS)
            .map(|_| {
                let mut data = [0u8; 100];
                rng.fill(&mut data[..]);
                set.add_element(&data);
                data
            })
            .collect();

        assert_ne!(set.finalize(), EMPTY_MUHASH);

        for elem in list.iter() {
            set.remove_element(elem);
        }

        assert_eq!(set.finalize(), EMPTY_MUHASH);
    }
}
