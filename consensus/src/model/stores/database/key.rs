use std::{fmt::Display, str};

const SEP: u8 = b'/';

#[derive(Debug, Clone)]
pub struct DbKey {
    path: Vec<u8>,
}

impl DbKey {
    pub fn new<TKey: Copy + AsRef<[u8]>>(prefix: &[u8], key: TKey) -> Self {
        Self { path: prefix.iter().chain(std::iter::once(&SEP)).chain(key.as_ref().iter()).copied().collect() }
    }

    pub fn prefix_only(prefix: &[u8]) -> Self {
        Self::new(prefix, b"")
    }
}

impl AsRef<[u8]> for DbKey {
    fn as_ref(&self) -> &[u8] {
        &self.path
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pos = self.path.len() - 1 - self.path.iter().rev().position(|c| *c == SEP).unwrap(); // Find the last position of `SEP`
        let (prefix, key) = (&self.path[..pos], &self.path[pos + 1..]);
        f.write_str(str::from_utf8(prefix).unwrap_or("{cannot display prefix}"))?;
        f.write_str("/")?;
        f.write_str(&faster_hex::hex_string(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashes::Hash;

    #[test]
    fn test_key_display() {
        let key1 = DbKey::new(b"human-readable", Hash::from_u64_word(34567890));
        let key2 = DbKey::new(&[1, 2, 2, 89], Hash::from_u64_word(345690));
        let key3 = DbKey::prefix_only(&[1, 2, 2, 89]);
        let key4 = DbKey::prefix_only(b"direct-prefix");
        println!("{}", key1);
        println!("{}", key2);
        println!("{}", key3);
        println!("{}", key4);
    }
}
