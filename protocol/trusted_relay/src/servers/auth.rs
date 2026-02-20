use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// HMAC‑SHA256 authentication for fast trusted relay peer connections.
///
/// Uses a shared secret to generate and validate tokens for each block,
/// and to compute per-packet MACs on the UDP data plane.
type HmacSha256 = Hmac<Sha256>;

/// Fixed-size authentication token (HMAC-SHA256 output = 32 bytes).
#[derive(Debug, Clone)]
pub struct AuthToken([u8; Self::TOKEN_SIZE]);

impl AuthToken {
    /// Token size in bytes (HMAC-SHA256 output).
    pub const TOKEN_SIZE: usize = 32;

    /// View the token as a byte slice.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Construct a token from a fixed-size array.
    #[inline]
    pub fn from_array(bytes: [u8; Self::TOKEN_SIZE]) -> Self {
        Self(bytes)
    }

    /// Construct a token from a byte vector.
    ///
    /// # Panics
    ///
    /// Panics if `bytes.len() != TOKEN_SIZE`.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let arr: [u8; Self::TOKEN_SIZE] = bytes.try_into().expect("AuthToken::from_bytes requires exactly 32 bytes");
        Self(arr)
    }
}

/// Generator and validator for HMAC‑SHA256 authentication tokens.
///
/// Cloneable so each worker thread can own its own copy.
#[derive(Clone)]
pub struct TokenAuthenticator {
    secret: Vec<u8>,
}

impl TokenAuthenticator {
    /// Create a new authenticator with the given shared secret.
    pub fn new(secret: Vec<u8>) -> Self {
        Self { secret }
    }

    /// Access the raw shared secret bytes.
    #[inline(always)]
    pub fn secret(&self) -> &[u8] {
        &self.secret
    }

    // -- HMAC helpers (private) -----------------------------------------------

    /// Create an HMAC instance keyed with the shared secret.
    ///
    /// SHA-256 HMAC accepts keys of any length, so `new_from_slice` cannot
    /// fail here. The `expect` is purely defensive.
    #[inline(always)]
    fn new_hmac(&self) -> HmacSha256 {
        HmacSha256::new_from_slice(&self.secret).expect("HMAC-SHA256 accepts keys of any length")
    }

    // -- Public API -----------------------------------------------------------

    /// Generate an authentication token for a block.
    ///
    /// Token = HMAC-SHA256(secret, block_hash ‖ SHA256(block_data)).
    #[inline(always)]
    pub fn generate_token(&self, block_hash: &[u8; 32], block_data: &[u8]) -> AuthToken {
        let data_hash = Sha256::digest(block_data);
        let mut mac = self.new_hmac();
        mac.update(block_hash);
        mac.update(&data_hash);
        AuthToken(mac.finalize().into_bytes().into())
    }

    /// Validate an authentication token in constant time.
    #[inline(always)]
    pub fn validate_token(&self, block_hash: &[u8; 32], block_data: &[u8], token: &AuthToken) -> bool {
        let expected = self.generate_token(block_hash, block_data);
        expected.as_bytes().ct_eq(token.as_bytes()).into()
    }

    /// Compute a per-packet MAC (HMAC-SHA256). Returns a fixed 32-byte array.
    #[inline(always)]
    pub fn mac(&self, data: &[u8]) -> [u8; 32] {
        let mut mac = self.new_hmac();
        mac.update(data);
        mac.finalize().into_bytes().into()
    }

    /// Verify a packet MAC in constant time.
    ///
    /// Zero-allocation: compares the HMAC output directly against `mac_bytes`.
    #[inline(always)]
    pub fn verify_mac(&self, data: &[u8], mac_bytes: &[u8]) -> bool {
        let mut hmac = self.new_hmac();
        hmac.update(data);
        let computed = hmac.finalize().into_bytes();
        computed.as_slice().ct_eq(mac_bytes).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_generation_and_validation() {
        let secret = b"shared-secret-key".to_vec();
        let auth = TokenAuthenticator::new(secret);

        let block_hash = [1u8; 32];
        let block_data = b"Block data content";

        let token = auth.generate_token(&block_hash, block_data);
        assert!(auth.validate_token(&block_hash, block_data, &token));
    }

    #[test]
    fn test_token_invalid_on_different_data() {
        let secret = b"shared-secret-key".to_vec();
        let auth = TokenAuthenticator::new(secret);

        let block_hash = [1u8; 32];
        let block_data1 = b"Block data content";
        let block_data2 = b"Different data";

        let token = auth.generate_token(&block_hash, block_data1);
        assert!(!auth.validate_token(&block_hash, block_data2, &token));
    }

    #[test]
    fn test_token_invalid_on_different_hash() {
        let secret = b"shared-secret-key".to_vec();
        let auth = TokenAuthenticator::new(secret);

        let block_hash1 = [1u8; 32];
        let mut block_hash2 = [1u8; 32];
        block_hash2[0] = 2;

        let block_data = b"Block data content";

        let token = auth.generate_token(&block_hash1, block_data);
        assert!(!auth.validate_token(&block_hash2, block_data, &token));
    }

    #[test]
    fn test_token_size() {
        assert_eq!(AuthToken::TOKEN_SIZE, 32);
    }

    #[test]
    fn test_token_deterministic() {
        let secret = b"shared-secret-key".to_vec();
        let auth = TokenAuthenticator::new(secret);

        let block_hash = [5u8; 32];
        let block_data = b"Deterministic test";

        let token1 = auth.generate_token(&block_hash, block_data);
        let token2 = auth.generate_token(&block_hash, block_data);

        assert_eq!(token1.as_bytes(), token2.as_bytes());
    }
}
