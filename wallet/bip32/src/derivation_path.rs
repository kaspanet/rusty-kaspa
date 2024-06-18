//! Derivation paths

use crate::{ChildNumber, Error, Result};
//use alloc::vec::{self, Vec};
use core::{
    fmt::{self, Display},
    str::FromStr,
};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// Prefix for all derivation paths.
const PREFIX: &str = "m";

/// Derivation paths within a hierarchical keyspace.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DerivationPath {
    path: Vec<ChildNumber>,
}

impl<'de> Deserialize<'de> for DerivationPath {
    fn deserialize<D>(deserializer: D) -> std::result::Result<DerivationPath, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DerivationPathVisitor;
        impl<'de> de::Visitor<'de> for DerivationPathVisitor {
            type Value = DerivationPath;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string containing list of permissions separated by a '+'")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                DerivationPath::from_str(value).map_err(|err| de::Error::custom(err.to_string()))
            }
            fn visit_borrowed_str<E>(self, v: &'de str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                DerivationPath::from_str(v).map_err(|err| de::Error::custom(err.to_string()))
            }
        }

        deserializer.deserialize_str(DerivationPathVisitor)
    }
}

impl Serialize for DerivationPath {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl DerivationPath {
    /// Iterate over the [`ChildNumber`] values in this derivation path.
    pub fn iter(&self) -> impl Iterator<Item = ChildNumber> + '_ {
        self.path.iter().cloned()
    }

    /// Is this derivation path empty? (i.e. the root)
    pub fn is_empty(&self) -> bool {
        self.path.is_empty()
    }

    /// Get the count of [`ChildNumber`] values in this derivation path.
    pub fn len(&self) -> usize {
        self.path.len()
    }

    /// Get the parent [`DerivationPath`] for the current one.
    ///
    /// Returns `None` if this is already the root path.
    pub fn parent(&self) -> Option<Self> {
        self.path.len().checked_sub(1).map(|n| {
            let mut parent = self.clone();
            parent.path.truncate(n);
            parent
        })
    }

    /// Push a [`ChildNumber`] onto an existing derivation path.
    pub fn push(&mut self, child_number: ChildNumber) {
        self.path.push(child_number)
    }
}

impl AsRef<[ChildNumber]> for DerivationPath {
    fn as_ref(&self) -> &[ChildNumber] {
        &self.path
    }
}

impl Display for DerivationPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(PREFIX)?;

        for child_number in self.iter() {
            write!(f, "/{}", child_number)?;
        }

        Ok(())
    }
}

impl Extend<ChildNumber> for DerivationPath {
    fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = ChildNumber>,
    {
        self.path.extend(iter);
    }
}

impl FromStr for DerivationPath {
    type Err = Error;

    fn from_str(path: &str) -> Result<DerivationPath> {
        let mut path = path.split('/');

        if path.next() != Some(PREFIX) {
            return Err(Error::String(format!("Derivation don't start with `{PREFIX}/`")));
        }

        Ok(DerivationPath { path: path.map(str::parse).collect::<Result<_>>()? })
    }
}

impl IntoIterator for DerivationPath {
    type Item = ChildNumber;
    type IntoIter = std::vec::IntoIter<ChildNumber>;

    fn into_iter(self) -> std::vec::IntoIter<ChildNumber> {
        self.path.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::DerivationPath;
    //use alloc::string::ToString;

    /// BIP32 test vectors
    // TODO(tarcieri): consolidate test vectors
    #[test]
    fn round_trip() {
        let path_m = "m";
        assert_eq!(path_m.parse::<DerivationPath>().unwrap().to_string(), path_m);

        let path_m_0 = "m/0";
        assert_eq!(path_m_0.parse::<DerivationPath>().unwrap().to_string(), path_m_0);

        let path_m_0_2147483647h = "m/0/2147483647'";
        assert_eq!(path_m_0_2147483647h.parse::<DerivationPath>().unwrap().to_string(), path_m_0_2147483647h);

        let path_m_0_2147483647h_1 = "m/0/2147483647'/1";
        assert_eq!(path_m_0_2147483647h_1.parse::<DerivationPath>().unwrap().to_string(), path_m_0_2147483647h_1);

        let path_m_0_2147483647h_1_2147483646h = "m/0/2147483647'/1/2147483646'";
        assert_eq!(
            path_m_0_2147483647h_1_2147483646h.parse::<DerivationPath>().unwrap().to_string(),
            path_m_0_2147483647h_1_2147483646h
        );

        let path_m_0_2147483647h_1_2147483646h_2 = "m/0/2147483647'/1/2147483646'/2";
        assert_eq!(
            path_m_0_2147483647h_1_2147483646h_2.parse::<DerivationPath>().unwrap().to_string(),
            path_m_0_2147483647h_1_2147483646h_2
        );
    }

    #[test]
    fn parent() {
        let path_m_0_2147483647h = "m/0/2147483647'".parse::<DerivationPath>().unwrap();
        let path_m_0 = path_m_0_2147483647h.parent().unwrap();
        assert_eq!("m/0", path_m_0.to_string());

        let path_m = path_m_0.parent().unwrap();
        assert_eq!("m", path_m.to_string());
        assert_eq!(path_m.parent(), None);
    }
}
