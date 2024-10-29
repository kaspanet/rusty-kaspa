use crate::registry::{DatabaseStorePrefixes, SEPARATOR};
use smallvec::SmallVec;
use std::fmt::{Debug, Display};

#[derive(Clone)]
pub struct DbKey {
    path: SmallVec<[u8; 36]>, // Optimized for the common case of { prefix byte || HASH (32 bytes) }
    prefix_len: usize,
}

impl DbKey {
    pub fn new<TKey>(prefix: &[u8], key: TKey) -> Self
    where
        TKey: Clone + AsRef<[u8]>,
    {
        Self { path: prefix.iter().chain(key.as_ref().iter()).copied().collect(), prefix_len: prefix.len() }
    }

    pub fn new_with_bucket<TKey, TBucket>(prefix: &[u8], bucket: TBucket, key: TKey) -> Self
    where
        TKey: Clone + AsRef<[u8]>,
        TBucket: Copy + AsRef<[u8]>,
    {
        let mut db_key = Self::prefix_only(prefix);
        db_key.add_bucket(bucket);
        db_key.add_key(key);
        db_key
    }

    pub fn prefix_only(prefix: &[u8]) -> Self {
        Self::new(prefix, [])
    }

    /// add a bucket to the DBkey, this adds to the prefix length
    pub fn add_bucket<TBucket>(&mut self, bucket: TBucket)
    where
        TBucket: Copy + AsRef<[u8]>,
    {
        self.path.extend(bucket.as_ref().iter().copied());
        self.prefix_len += bucket.as_ref().len();
    }

    pub fn add_key<TKey>(&mut self, key: TKey)
    where
        TKey: Clone + AsRef<[u8]>,
    {
        self.path.extend(key.as_ref().iter().copied());
        self.prefix_len += key.as_ref().len();
    }

    pub fn prefix_len(&self) -> usize {
        self.prefix_len
    }
}

impl AsRef<[u8]> for DbKey {
    fn as_ref(&self) -> &[u8] {
        &self.path
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use DatabaseStorePrefixes::*;
        let mut pos = 0;

        if self.prefix_len > 0 {
            if let Ok(prefix) = DatabaseStorePrefixes::try_from(self.path[0]) {
                prefix.fmt(f)?;
                f.write_str("/")?;
                pos += 1;
                if self.prefix_len > 1 {
                    match prefix {
                        Ghostdag
                        | GhostdagCompact
                        | TempGhostdag
                        | TempGhostdagCompact
                        | RelationsParents
                        | RelationsChildren
                        | Reachability
                        | ReachabilityTreeChildren
                        | ReachabilityFutureCoveringSet => {
                            if self.path[1] != SEPARATOR {
                                // Expected to be a block level so we display as a number
                                Display::fmt(&self.path[1], f)?;
                                f.write_str("/")?;
                            }
                            pos += 1;
                        }
                        ReachabilityRelations => {
                            if let Ok(next_prefix) = DatabaseStorePrefixes::try_from(self.path[1]) {
                                next_prefix.fmt(f)?;
                                f.write_str("/")?;
                                pos += 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // We expect that the key part is usually more readable as hex
        f.write_str(&faster_hex::hex_string(&self.path[pos..]))
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
    use kaspa_hashes::{Hash, HASH_SIZE};
    use DatabaseStorePrefixes::*;

    #[test]
    fn test_key_display() {
        let level = 37;
        let key1 = DbKey::new(&[ReachabilityRelations.into(), RelationsParents.into()], Hash::from_u64_word(34567890));
        let key2 = DbKey::new(&[Reachability.into(), Separator.into()], Hash::from_u64_word(345690));
        let key3 = DbKey::new(&[Reachability.into(), level], Hash::from_u64_word(345690));
        let key4 = DbKey::new(&[RelationsParents.into(), level], Hash::from_u64_word(345690));

        assert!(key1.to_string().starts_with(&format!("{:?}/{:?}/00", ReachabilityRelations, RelationsParents)));
        assert!(key2.to_string().starts_with(&format!("{:?}/00", Reachability)));
        assert!(key3.to_string().starts_with(&format!("{:?}/{}/00", Reachability, level)));
        assert!(key4.to_string().starts_with(&format!("{:?}/{}/00", RelationsParents, level)));

        let key5 = DbKey::new(b"human/readable", Hash::from_bytes([SEPARATOR; HASH_SIZE]));
        let key6 = DbKey::prefix_only(&[0xC0, 0xC1, 0xF5, 0xF6]);
        let key7 = DbKey::prefix_only(b"direct-prefix");

        // Make sure display can handle arbitrary strings
        let _ = key5.to_string();
        let _ = key6.to_string();
        let _ = key7.to_string();
    }
}
