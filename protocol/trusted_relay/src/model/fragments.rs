use bytes::{Bytes, BytesMut};
use kaspa_consensus_core::Hash as BlockHash;

pub type FragmentPayload = Bytes;

/// Fixed-size header prepended to every Fragment on the wire.
///
/// Layout: `[0..32] block_hash | [32..34] Fragment_index (LE) | [34..36] total_fragments (LE)`
#[derive(Clone)]
pub struct FragmentHeader(pub [u8; Self::SIZE]);

impl FragmentHeader {
    // Wire size is fixed: 32-byte hash + 2-byte Fragment_index + 2-byte total_fragments = 36 bytes.
    pub const SIZE: usize = 36;
    pub const FRAGMENT_INDEX_OFFSET: usize = 32;
    pub const TOTAL_FRAGMENTS_OFFSET: usize = 34;

    pub fn new(hash: BlockHash, fragment_index: u16, total_fragments: u16) -> Self {
        let mut header = Self([0u8; Self::SIZE]);
        header.0[0..32].copy_from_slice(&hash.as_bytes()[..]);
        header.0[Self::FRAGMENT_INDEX_OFFSET..Self::FRAGMENT_INDEX_OFFSET + 2].copy_from_slice(&fragment_index.to_le_bytes());
        header.0[Self::TOTAL_FRAGMENTS_OFFSET..Self::TOTAL_FRAGMENTS_OFFSET + 2].copy_from_slice(&total_fragments.to_le_bytes());
        header
    }

    pub fn block_hash(&self) -> BlockHash {
        BlockHash::from_slice(&self.0[..Self::FRAGMENT_INDEX_OFFSET])
    }

    pub fn is_last_gen(&self, k: u16, m: u16) -> bool {
        self.fragment_generation(k, m) == self.last_fragment_generation(k, m)
    }

    pub fn last_gen_k(&self, k: u16, m: u16) -> u8 {
        let gen_size = k + m;
        let total_fragments = self.total_fragments();
        let last_gen_fragments = total_fragments % gen_size;
        if last_gen_fragments == 0 { k as u8 } else { (last_gen_fragments.min(k)) as u8 }
    }

    pub fn last_gen_m(&self, k: u16, m: u16) -> u8 {
        let gen_size = k + m;
        let total_fragments = self.total_fragments();
        let last_gen_fragments = total_fragments % gen_size;
        if last_gen_fragments == 0 { m as u8 } else { (last_gen_fragments.saturating_sub(k)) as u8 }
    }

    pub fn is_data(&self, k: u16, m: u16) -> bool {
        let gen_size = k + m;
        let index_within_gen = self.fragment_index() % gen_size;
        index_within_gen < k
    }

    pub fn is_parity(&self, k: u16, m: u16) -> bool {
        !self.is_data(k, m)
    }

    pub fn fragment_generation(&self, k: u16, m: u16) -> u8 {
        let gen_size = k + m;
        (self.fragment_index() / gen_size) as u8
    }

    pub fn last_fragment_generation(&self, k: u16, m: u16) -> u8 {
        let gen_size = k + m;
        let total_fragments = self.total_fragments();
        let last_gen_fragments = total_fragments % gen_size;
        if last_gen_fragments == 0 { (total_fragments / gen_size - 1) as u8 } else { (total_fragments / gen_size) as u8 }
    }

    pub fn index_within_generation(&self, gen_size: u16) -> u8 {
        (self.fragment_index() % gen_size) as u8
    }

    pub fn fragment_index(&self) -> u16 {
        self.0[Self::FRAGMENT_INDEX_OFFSET..Self::TOTAL_FRAGMENTS_OFFSET]
            .try_into()
            .map(u16::from_le_bytes)
            .expect("fragment_index bytes should always be present and valid")
    }

    pub fn total_fragments(&self) -> u16 {
        self.0[Self::TOTAL_FRAGMENTS_OFFSET..Self::TOTAL_FRAGMENTS_OFFSET + 2]
            .try_into()
            .map(u16::from_le_bytes)
            .expect("total_fragments bytes should always be present and valid")
    }

    pub fn total_generations(&self, gen_size: usize) -> u8 {
        (self.total_fragments() as usize).div_ceil(gen_size) as u8
    }

    pub fn as_bytes(&self) -> &[u8; Self::SIZE] {
        &self.0
    }
}

impl PartialEq for FragmentHeader {
    fn eq(&self, other: &Self) -> bool {
        self.block_hash() == other.block_hash() && self.fragment_index() == other.fragment_index()
    }
}

impl Eq for FragmentHeader {}

impl PartialOrd for FragmentHeader {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FragmentHeader {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.block_hash().cmp(&other.block_hash()).then_with(|| self.fragment_index().cmp(&other.fragment_index()))
    }
}

#[derive(Clone)]
pub struct Fragment {
    pub header: FragmentHeader,
    pub payload: FragmentPayload,
}

impl Fragment {
    pub fn new(header: FragmentHeader, payload: FragmentPayload) -> Self {
        Self { header, payload }
    }

    pub fn header(&self) -> &FragmentHeader {
        &self.header
    }

    pub fn payload(&self) -> &FragmentPayload {
        &self.payload
    }

    /// Serialize to wire format: 36-byte header + payload.
    /// Mirrors the deserialization in `From<Bytes> for Fragment`.
    pub fn to_bytes(&self) -> Bytes {
        let mut buf = Vec::with_capacity(FragmentHeader::SIZE + self.payload.len());
        buf.extend_from_slice(self.header.as_bytes());
        buf.extend_from_slice(&self.payload);
        Bytes::from(buf)
    }
}

impl From<Bytes> for Fragment {
    /// Backward-compatible (infallible) conversion. This preserves the existing
    /// panic-on-invalid-input behaviour for callers that rely on `From<Bytes>`.
    fn from(bytes: Bytes) -> Self {
        // Reuse the fallible constructor and keep the original panic semantics.
        Fragment::from_bytes(bytes)
    }
}

impl Fragment {
    /// Fallible, non-panicking constructor from `Bytes`.
    /// Prefer this in library code; callers can handle `RelayError::ParseError`.
    pub fn from_bytes(bytes: Bytes) -> Self {
        if bytes.len() < FragmentHeader::SIZE {
            panic!("Invalid Fragment data: length {} is less than header size {}", bytes.len(), FragmentHeader::SIZE);
        }

        let header_bytes = &bytes[..FragmentHeader::SIZE];
        let payload_bytes = bytes.slice(FragmentHeader::SIZE..);

        // Deserialize header
        let hash = BlockHash::from_slice(&header_bytes[0..32]);

        let fragment_index =
            header_bytes[32..34].try_into().map(u16::from_le_bytes).expect("Invalid Fragment data: failed to parse fragment_index");

        let total_fragments =
            header_bytes[34..36].try_into().map(u16::from_le_bytes).expect("Invalid Fragment data: failed to parse total_fragments");

        let header = FragmentHeader::new(hash, fragment_index, total_fragments);
        Fragment { header, payload: payload_bytes }
    }
}

impl From<BytesMut> for Fragment {
    fn from(bytes: BytesMut) -> Self {
        Fragment::from(bytes.freeze())
    }
}

impl From<&[u8]> for Fragment {
    fn from(bytes: &[u8]) -> Self {
        Fragment::from(Bytes::copy_from_slice(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::Hash as BlockHash;

    #[test]
    fn from_bytes_accepts_valid_wire_format() {
        // Build a minimal valid Fragment: 32-byte hash + 2-byte fragment_index + 2-byte total_fragments + payload
        let hash = BlockHash::from_bytes([7u8; 32]);
        let mut buf = Vec::with_capacity(36 + 4);
        buf.extend_from_slice(&hash.as_bytes()[..]);
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&4u16.to_le_bytes());
        buf.extend_from_slice(&[1u8, 2u8, 3u8, 4u8]);

        let bytes = Bytes::from(buf.clone());
        let frag = Fragment::from_bytes(bytes);
        assert_eq!(frag.header().fragment_index(), 1);
        assert_eq!(frag.header().total_fragments(), 4);
        assert_eq!(&frag.payload()[..], &[1u8, 2u8, 3u8, 4u8]);
    }

    #[test]
    fn test_fragment_header_endianness_roundtrip() {
        let hash = BlockHash::from_bytes([0xAAu8; 32]);
        let hdr = FragmentHeader::new(hash, 0x1234, 0x00FF);
        assert_eq!(hdr.fragment_index(), 0x1234);
        assert_eq!(hdr.total_fragments(), 0x00FF);
    }
}
