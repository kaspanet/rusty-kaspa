use zeroize::Zeroize;

#[derive(Clone)]
pub struct Secret(Vec<u8>);

impl Secret {
    pub fn new(data: Vec<u8>) -> Self {
        Self(data)
    }
}

impl AsRef<[u8]> for Secret {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for Secret {
    fn from(vec: Vec<u8>) -> Self {
        Secret(vec)
    }
}
impl From<&[u8]> for Secret {
    fn from(slice: &[u8]) -> Self {
        Secret(slice.to_vec())
    }
}
impl From<&str> for Secret {
    fn from(s: &str) -> Self {
        Secret(s.trim().as_bytes().to_vec())
    }
}

impl From<String> for Secret {
    fn from(mut s: String) -> Self {
        let secret = Secret(s.trim().as_bytes().to_vec());
        s.zeroize();
        secret
    }
}

impl Zeroize for Secret {
    fn zeroize(&mut self) {
        self.0.zeroize()
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.zeroize()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secret").field("secret", &"********").finish()
    }
}
