//! BIP39 seed values

use zeroize::Zeroize;

/// BIP39 seeds.
// TODO(tarcieri): support for 32-byte seeds
#[cfg_attr(docsrs, doc(cfg(feature = "bip39")))]
pub struct Seed(pub(crate) [u8; Seed::SIZE]);

impl Seed {
    /// Number of bytes of PBKDF2 output to extract.
    pub const SIZE: usize = 64;

    /// Create a new seed from the given bytes.
    pub fn new(bytes: [u8; Seed::SIZE]) -> Self {
        Seed(bytes)
    }

    /// Get the inner secret byte slice
    pub fn as_bytes(&self) -> &[u8; Seed::SIZE] {
        &self.0
    }
}

impl AsRef<[u8]> for Seed {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl Drop for Seed {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
