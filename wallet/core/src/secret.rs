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
        Secret(s.as_bytes().to_vec())
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize()
    }
}
