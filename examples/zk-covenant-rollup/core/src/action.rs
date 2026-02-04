//! Action types for account-based rollup.
//!
//! Actions are determined by operation code (u16). Each action type
//! has its own set of arguments that follow the header.

/// Operation codes
pub const OP_TRANSFER: u16 = 0;

/// Current action version
pub const ACTION_VERSION: u16 = 1;

/// Action header (common to all actions)
///
/// Layout:
/// - version: u16 (2 bytes)
/// - operation: u16 (2 bytes) - determines action type and data size
/// - nonce: u32 (4 bytes) - for tx_id matching
///
/// Total: 8 bytes = 2 words
///
/// After reading the header, read action-specific data based on operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct ActionHeader {
    /// Format version
    pub version: u16,
    /// Operation type - determines what data follows
    pub operation: u16,
    /// Nonce for tx_id matching
    pub nonce: u32,
}

impl ActionHeader {
    /// Size in bytes
    pub const SIZE: usize = core::mem::size_of::<Self>();

    /// Size in u32 words
    pub const WORDS: usize = Self::SIZE / 4;

    /// Create a new header
    pub fn new(operation: u16, nonce: u32) -> Self {
        Self { version: ACTION_VERSION, operation, nonce }
    }

    /// Check if version is valid
    pub fn is_valid_version(&self) -> bool {
        self.version == ACTION_VERSION
    }

    /// Get the size of action data that follows this header
    pub fn data_size(&self) -> Option<usize> {
        match self.operation {
            OP_TRANSFER => Some(TransferAction::SIZE),
            _ => None,
        }
    }

    /// Convert to word slice
    pub fn as_words(&self) -> &[u32] {
        bytemuck::cast_slice(bytemuck::bytes_of(self))
    }

    /// Convert from word slice
    pub fn from_words(words: [u32; Self::WORDS]) -> Self {
        bytemuck::cast(words)
    }

    pub fn from_words_ref(words: &[u32; Self::WORDS]) -> &Self {
        bytemuck::cast_ref(words)
    }
}

/// Transfer action data (follows ActionHeader when operation == OP_TRANSFER)
///
/// Layout:
/// - source: [u32; 8] (32 bytes) - sender pubkey
/// - destination: [u32; 8] (32 bytes) - recipient pubkey
/// - amount: u64 (8 bytes)
///
/// Total: 72 bytes = 18 words
#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct TransferAction {
    /// Sender pubkey (committed to tx_id via payload_digest)
    pub source: [u32; 8],
    /// Recipient pubkey
    pub destination: [u32; 8],
    /// Amount to transfer
    pub amount: u64,
}

impl TransferAction {
    /// Size in bytes
    pub const SIZE: usize = core::mem::size_of::<Self>();

    /// Size in u32 words
    pub const WORDS: usize = Self::SIZE / 4;

    /// Create a new transfer action
    pub fn new(source: [u32; 8], destination: [u32; 8], amount: u64) -> Self {
        Self { source, destination, amount }
    }

    /// Check if this transfer is valid
    pub fn is_valid(&self) -> bool {
        self.amount > 0
    }

    /// Convert to word slice
    pub fn as_words(&self) -> &[u32] {
        bytemuck::cast_slice(bytemuck::bytes_of(self))
    }

    /// Convert from word slice
    pub fn from_words(words: [u32; Self::WORDS]) -> Self {
        bytemuck::cast(words)
    }
}

/// Parsed action types
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Transfer(TransferAction),
}

impl Action {
    /// Get the source pubkey (for authorization verification)
    pub fn source(&self) -> [u32; 8] {
        match self {
            Action::Transfer(t) => t.source,
        }
    }

    /// Check if the action is valid
    pub fn is_valid(&self) -> bool {
        match self {
            Action::Transfer(t) => t.is_valid(),
        }
    }

    /// Get as transfer action if it is one
    pub fn as_transfer(&self) -> Option<&TransferAction> {
        match self {
            Action::Transfer(t) => Some(t),
        }
    }
}

/// Full action payload size (header + transfer data)
/// Used for computing payload_digest
pub const TRANSFER_PAYLOAD_SIZE: usize = ActionHeader::SIZE + TransferAction::SIZE;
pub const TRANSFER_PAYLOAD_WORDS: usize = TRANSFER_PAYLOAD_SIZE / 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_header_size() {
        // 2 + 2 + 4 = 8 bytes
        assert_eq!(ActionHeader::SIZE, 8);
        assert_eq!(ActionHeader::WORDS, 2);
    }

    #[test]
    fn test_transfer_action_size() {
        // 32 + 32 + 8 = 72 bytes
        assert_eq!(TransferAction::SIZE, 72);
        assert_eq!(TransferAction::WORDS, 18);
    }

    #[test]
    fn test_transfer_payload_size() {
        // 8 + 72 = 80 bytes = 20 words
        assert_eq!(TRANSFER_PAYLOAD_SIZE, 80);
        assert_eq!(TRANSFER_PAYLOAD_WORDS, 20);
    }

    #[test]
    fn test_header_data_size() {
        let header = ActionHeader::new(OP_TRANSFER, 0);
        assert_eq!(header.data_size(), Some(TransferAction::SIZE));

        let unknown = ActionHeader { version: ACTION_VERSION, operation: 99, nonce: 0 };
        assert_eq!(unknown.data_size(), None);
    }

    #[test]
    fn test_transfer_valid() {
        let transfer = TransferAction::new([1; 8], [2; 8], 100);
        assert!(transfer.is_valid());

        let zero_amount = TransferAction::new([1; 8], [2; 8], 0);
        assert!(!zero_amount.is_valid());
    }

    #[test]
    fn test_header_roundtrip() {
        let header = ActionHeader::new(OP_TRANSFER, 12345);
        let words: [u32; ActionHeader::WORDS] = bytemuck::cast(header);
        let restored = ActionHeader::from_words(words);
        assert_eq!(header, restored);
    }

    #[test]
    fn test_transfer_roundtrip() {
        let transfer = TransferAction::new([0x11111111; 8], [0x22222222; 8], 999999);
        let words: [u32; TransferAction::WORDS] = bytemuck::cast(transfer);
        let restored = TransferAction::from_words(words);
        assert_eq!(transfer, restored);
    }
}
