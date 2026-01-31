use kaspa_utils::flattened_slice::{FlattenedSliceBuilder, PayloadPrefixFilter};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::ops::Deref;

/// RPC-layer wrapper around [`PayloadPrefixFilter`] that adds
/// `Serialize`, `Deserialize`, `BorshSerialize`, `BorshDeserialize`, and `Display`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RpcPayloadPrefixFilter(pub PayloadPrefixFilter);

impl RpcPayloadPrefixFilter {
    /// Create from raw flattened data and slice lengths.
    pub fn new(flattened_data: Vec<u8>, slice_lengths: Vec<u32>) -> Self {
        Self(PayloadPrefixFilter::from_raw(flattened_data, slice_lengths))
    }

    /// Build from a `Vec<Vec<u8>>` of prefixes.
    pub fn from_prefixes(prefixes: Vec<Vec<u8>>) -> Self {
        Self(PayloadPrefixFilter::from_prefixes(prefixes))
    }

    /// Build from a slice of prefixes.
    pub fn from_prefixes_ref(prefixes: &[Vec<u8>]) -> Self {
        Self(PayloadPrefixFilter::from_prefixes_ref(prefixes))
    }

    /// Reconstruct as `Vec<Vec<u8>>` by iterating the holder.
    pub fn to_vec(&self) -> Vec<Vec<u8>> {
        self.0.as_holder().iter().map(|s| s.to_vec()).collect()
    }
}

impl Deref for RpcPayloadPrefixFilter {
    type Target = PayloadPrefixFilter;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<PayloadPrefixFilter> for RpcPayloadPrefixFilter {
    fn from(inner: PayloadPrefixFilter) -> Self {
        Self(inner)
    }
}

impl fmt::Display for RpcPayloadPrefixFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.0.len();
        match count {
            0 => write!(f, "all"),
            1 => write!(f, "1 prefix"),
            n => write!(f, "{} prefixes", n),
        }
    }
}

// Serialize the two inner vectors directly (flattened_data + slice_lengths)
impl Serialize for RpcPayloadPrefixFilter {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.0.flattened_data(), self.0.slice_lengths()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RpcPayloadPrefixFilter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (flattened_data, slice_lengths): (Vec<u8>, Vec<u32>) = Deserialize::deserialize(deserializer)?;
        Ok(Self(PayloadPrefixFilter::from_raw(flattened_data, slice_lengths)))
    }
}

impl borsh::BorshSerialize for RpcPayloadPrefixFilter {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        borsh::BorshSerialize::serialize(self.0.flattened_data(), writer)?;
        borsh::BorshSerialize::serialize(self.0.slice_lengths(), writer)
    }
}

impl borsh::BorshDeserialize for RpcPayloadPrefixFilter {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let flattened_data: Vec<u8> = borsh::BorshDeserialize::deserialize_reader(reader)?;
        let slice_lengths: Vec<u32> = borsh::BorshDeserialize::deserialize_reader(reader)?;
        Ok(Self(PayloadPrefixFilter::from_raw(flattened_data, slice_lengths)))
    }
}

impl<'a> FromIterator<&'a [u8]> for RpcPayloadPrefixFilter {
    fn from_iter<T: IntoIterator<Item = &'a [u8]>>(iter: T) -> Self {
        FlattenedSliceBuilder::from_iter(iter).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_prefixes() {
        let prefixes = vec![vec![0xAA, 0xBB], vec![0xCC, 0xDD, 0xEE]];
        let filter = RpcPayloadPrefixFilter::from_prefixes(prefixes.clone());
        assert_eq!(filter.to_vec(), prefixes);
        assert_eq!(filter.len(), 2);
        assert!(!filter.is_empty());
    }

    #[test]
    fn test_empty() {
        let filter = RpcPayloadPrefixFilter::default();
        assert!(filter.is_empty());
        assert_eq!(filter.len(), 0);
        assert_eq!(filter.to_vec(), Vec::<Vec<u8>>::new());
    }

    #[test]
    fn test_contains_prefix() {
        let filter = RpcPayloadPrefixFilter::from_prefixes(vec![vec![0xAA, 0xBB], vec![0xCC]]);
        assert!(filter.contains_prefix(&[0xAA, 0xBB, 0x01, 0x02]));
        assert!(filter.contains_prefix(&[0xCC, 0xDD]));
        assert!(!filter.contains_prefix(&[0xDD, 0xEE]));
        assert!(filter.contains_prefix(&[0xAA, 0xBB])); // exact match
    }

    #[test]
    fn test_display() {
        assert_eq!(RpcPayloadPrefixFilter::default().to_string(), "all");
        assert_eq!(RpcPayloadPrefixFilter::from_prefixes(vec![vec![1]]).to_string(), "1 prefix");
        assert_eq!(RpcPayloadPrefixFilter::from_prefixes(vec![vec![1], vec![2]]).to_string(), "2 prefixes");
    }

    #[test]
    fn test_new_raw() {
        let filter = RpcPayloadPrefixFilter::new(vec![0xAA, 0xBB, 0xCC], vec![2, 1]);
        assert_eq!(filter.to_vec(), vec![vec![0xAA, 0xBB], vec![0xCC]]);
    }

    #[test]
    fn test_deref() {
        let filter = RpcPayloadPrefixFilter::from_prefixes(vec![vec![1, 2], vec![3]]);
        // Access PayloadPrefixFilter methods via Deref
        let holder = filter.as_holder();
        let slices: Vec<&[u8]> = holder.iter().collect();
        assert_eq!(slices, vec![&[1, 2][..], &[3][..]]);
    }

    #[test]
    fn test_from_payload_prefix_filter() {
        let inner = PayloadPrefixFilter::from_prefixes(vec![vec![0xAA]]);
        let filter: RpcPayloadPrefixFilter = inner.into();
        assert_eq!(filter.to_vec(), vec![vec![0xAA]]);
    }

    #[test]
    fn test_borsh_roundtrip() {
        let filter = RpcPayloadPrefixFilter::from_prefixes(vec![vec![0xAA, 0xBB], vec![0xCC]]);
        let mut buf = Vec::new();
        borsh::BorshSerialize::serialize(&filter, &mut buf).unwrap();
        let deserialized: RpcPayloadPrefixFilter = borsh::BorshDeserialize::deserialize(&mut buf.as_slice()).unwrap();
        assert_eq!(filter, deserialized);
    }
}
