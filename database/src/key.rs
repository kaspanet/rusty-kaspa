use std::{
    fmt::{Debug, Display},
    str,
};

const SEP: u8 = b'/';

#[derive(Clone)]
pub struct DbKey {
    path: Vec<u8>,
    prefix_len: usize,
}

impl DbKey {
    pub fn new<TKey: Copy + AsRef<[u8]>>(prefix: &[u8], key: TKey) -> Self {
        Self {
            path: prefix.iter().chain(std::iter::once(&SEP)).chain(key.as_ref().iter()).copied().collect(),
            prefix_len: prefix.len() + 1, // Include `SEP` as part of the prefix
        }
    }

    pub fn prefix_only(prefix: &[u8]) -> Self {
        Self::new(prefix, [])
    }
}

impl AsRef<[u8]> for DbKey {
    fn as_ref(&self) -> &[u8] {
        &self.path
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (prefix, key) = self.path.split_at(self.prefix_len);
        // We expect the prefix to be human readable
        if let Ok(s) = str::from_utf8(prefix) {
            f.write_str(s)?;
        } else {
            // Otherwise we fallback to hex parsing
            f.write_str(&faster_hex::hex_string(&prefix[..prefix.len() - 1]))?; // Drop `SEP`
            f.write_str("/")?;
        }
        // We expect that key is usually more readable as hex
        f.write_str(&faster_hex::hex_string(key))
    }
}

impl Debug for DbKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashes::{Hash, HASH_SIZE};

    #[test]
    fn test_key_display() {
        let key1 = DbKey::new(b"human-readable", Hash::from_u64_word(34567890));
        let key2 = DbKey::new(&[0xC0, 0xC1, 0xF5, 0xF6], Hash::from_u64_word(345690)); // `0xC0, 0xC1, 0xF5, 0xF6` are invalid UTF-8 chars
        let key3 = DbKey::new(b"human/readable", Hash::from_bytes([SEP; HASH_SIZE])); // Add many binary `/` to assert prefix is recognized
        let key4 = DbKey::prefix_only(&[0xC0, 0xC1, 0xF5, 0xF6]);
        let key5 = DbKey::prefix_only(b"direct-prefix");

        assert!(key1.to_string().starts_with("human-readable/"));
        assert!(key2.to_string().starts_with("c0c1f5f6/")); // Expecting hex since invalid UTF-8 was used
        assert!(key3.to_string().starts_with("human/readable/"));
        assert_eq!(key4.to_string(), "c0c1f5f6/");
        assert_eq!(key5.to_string(), "direct-prefix/");
    }
}
