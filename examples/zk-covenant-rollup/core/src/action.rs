//! Action types for account-based rollup.
//!
//! Actions are determined by operation code (u16). Each action type
//! has its own set of arguments that follow the header.

/// Operation codes
pub const OP_TRANSFER: u16 = 0;
pub const OP_ENTRY: u16 = 1;
pub const OP_EXIT: u16 = 2;

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
            OP_ENTRY => Some(EntryAction::SIZE),
            OP_EXIT => Some(ExitAction::SIZE),
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

/// Entry (deposit) action data (follows ActionHeader when operation == OP_ENTRY)
///
/// Layout:
/// - destination: [u32; 8] (32 bytes) - recipient pubkey on L2
///
/// Total: 32 bytes = 8 words
///
/// The deposit amount is NOT in the payload — it comes from the tx output value,
/// verified by the guest using the rest_preimage.
/// Destination can be zeros (no validation on zero pubkey).
#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct EntryAction {
    /// Recipient pubkey on L2
    pub destination: [u32; 8],
}

impl EntryAction {
    /// Size in bytes
    pub const SIZE: usize = core::mem::size_of::<Self>();

    /// Size in u32 words
    pub const WORDS: usize = Self::SIZE / 4;

    /// Create a new entry action
    pub fn new(destination: [u32; 8]) -> Self {
        Self { destination }
    }

    /// Entry is always valid (destination can be any value including zeros)
    pub fn is_valid(&self) -> bool {
        true
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

/// Maximum destination SPK size in the exit action payload (35 bytes).
/// Fits P2PK (34B) and P2SH (35B). Actual length is inferred from SPK content
/// (P2PK starts with OP_DATA_32=0x20, P2SH starts with OP_BLAKE2B=0xaa).
pub const EXIT_SPK_MAX: usize = 35;

/// Word count for the SPK + padding region: 35 bytes SPK + 5 bytes zero padding = 40 bytes = 10 words.
pub const EXIT_SPK_WORDS: usize = 10;

/// Exit (withdrawal) action data (follows ActionHeader when operation == OP_EXIT)
///
/// Layout:
/// - source: [u32; 8] (32 bytes) - sender L2 pubkey (authorized via prev tx output)
/// - destination_spk: [u32; 10] (40 bytes) - L1 SPK in first 35 bytes, last 5 zero-padded
/// - amount: u64 (8 bytes) - amount to withdraw
///
/// Total: 80 bytes = 20 words
#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct ExitAction {
    /// Sender L2 pubkey (authorized via prev tx output, same as TransferAction)
    pub source: [u32; 8],
    /// Destination L1 SPK stored as words. First 35 bytes = SPK, last 5 = zero padding.
    pub destination_spk: [u32; EXIT_SPK_WORDS],
    /// Amount to withdraw
    pub amount: u64,
}

impl ExitAction {
    /// Size in bytes
    pub const SIZE: usize = core::mem::size_of::<Self>();

    /// Size in u32 words
    pub const WORDS: usize = Self::SIZE / 4;

    /// Create a new exit action
    pub fn new(source: [u32; 8], destination_spk: &[u8], amount: u64) -> Self {
        assert!(destination_spk.len() <= EXIT_SPK_MAX);
        let mut spk_words = [0u32; EXIT_SPK_WORDS];
        let spk_bytes: &mut [u8] = bytemuck::cast_slice_mut(&mut spk_words);
        spk_bytes[..destination_spk.len()].copy_from_slice(destination_spk);
        Self { source, destination_spk: spk_words, amount }
    }

    /// Infer the actual SPK length from its content.
    /// Schnorr P2PK starts with OP_DATA_32 (0x20) → 34 bytes.
    /// ECDSA P2PK starts with OP_DATA_33 (0x21) → 35 bytes.
    /// P2SH starts with OP_BLAKE2B (0xaa) → 35 bytes.
    pub fn spk_len(&self) -> usize {
        let first_byte = self.destination_spk[0] as u8;
        if first_byte == 0x20 { 34 } else { 35 }
    }

    /// Get the actual destination SPK bytes (trimmed to inferred length)
    pub fn destination_spk_bytes(&self) -> &[u8] {
        let bytes: &[u8] = bytemuck::cast_slice(&self.destination_spk);
        &bytes[..self.spk_len()]
    }

    /// Check if this exit is valid
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
    Entry(EntryAction),
    Exit(ExitAction),
}

impl Action {
    /// Get the source pubkey (for authorization verification).
    /// Returns `None` for Entry actions (no source — deposits come from L1).
    pub fn source(&self) -> Option<[u32; 8]> {
        match self {
            Action::Transfer(t) => Some(t.source),
            Action::Entry(_) => None,
            Action::Exit(e) => Some(e.source),
        }
    }

    /// Check if the action is valid
    pub fn is_valid(&self) -> bool {
        match self {
            Action::Transfer(t) => t.is_valid(),
            Action::Entry(e) => e.is_valid(),
            Action::Exit(e) => e.is_valid(),
        }
    }

    /// Get as transfer action if it is one
    pub fn as_transfer(&self) -> Option<&TransferAction> {
        match self {
            Action::Transfer(t) => Some(t),
            _ => None,
        }
    }

    /// Get as entry action if it is one
    pub fn as_entry(&self) -> Option<&EntryAction> {
        match self {
            Action::Entry(e) => Some(e),
            _ => None,
        }
    }

    /// Get as exit action if it is one
    pub fn as_exit(&self) -> Option<&ExitAction> {
        match self {
            Action::Exit(e) => Some(e),
            _ => None,
        }
    }
}

/// Full transfer payload size: header (8 bytes) + TransferAction (72 bytes) = 80 bytes = 20 words
pub const TRANSFER_PAYLOAD_SIZE: usize = ActionHeader::SIZE + TransferAction::SIZE;
pub const TRANSFER_PAYLOAD_WORDS: usize = TRANSFER_PAYLOAD_SIZE / 4;

/// Full entry payload size: header (8 bytes) + EntryAction (32 bytes) = 40 bytes = 10 words
pub const ENTRY_PAYLOAD_SIZE: usize = ActionHeader::SIZE + EntryAction::SIZE;
pub const ENTRY_PAYLOAD_WORDS: usize = ENTRY_PAYLOAD_SIZE / 4;

/// Full exit payload size: header (8 bytes) + ExitAction (80 bytes) = 88 bytes = 22 words
pub const EXIT_PAYLOAD_SIZE: usize = ActionHeader::SIZE + ExitAction::SIZE;
pub const EXIT_PAYLOAD_WORDS: usize = EXIT_PAYLOAD_SIZE / 4;

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
    fn test_entry_action_size() {
        // 32 bytes = 8 words
        assert_eq!(EntryAction::SIZE, 32);
        assert_eq!(EntryAction::WORDS, 8);
    }

    #[test]
    fn test_transfer_payload_size() {
        // 8 + 72 = 80 bytes = 20 words
        assert_eq!(TRANSFER_PAYLOAD_SIZE, 80);
        assert_eq!(TRANSFER_PAYLOAD_WORDS, 20);
    }

    #[test]
    fn test_entry_payload_size() {
        // 8 + 32 = 40 bytes = 10 words
        assert_eq!(ENTRY_PAYLOAD_SIZE, 40);
        assert_eq!(ENTRY_PAYLOAD_WORDS, 10);
    }

    #[test]
    fn test_header_data_size() {
        let header = ActionHeader::new(OP_TRANSFER, 0);
        assert_eq!(header.data_size(), Some(TransferAction::SIZE));

        let entry_header = ActionHeader::new(OP_ENTRY, 0);
        assert_eq!(entry_header.data_size(), Some(EntryAction::SIZE));

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
    fn test_entry_valid() {
        let entry = EntryAction::new([1; 8]);
        assert!(entry.is_valid());

        // Zero destination is also valid
        let zero_dest = EntryAction::new([0; 8]);
        assert!(zero_dest.is_valid());
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

    #[test]
    fn test_entry_roundtrip() {
        let entry = EntryAction::new([0xDEADBEEF; 8]);
        let words: [u32; EntryAction::WORDS] = bytemuck::cast(entry);
        let restored = EntryAction::from_words(words);
        assert_eq!(entry, restored);
    }

    #[test]
    fn test_action_source() {
        let transfer = Action::Transfer(TransferAction::new([1; 8], [2; 8], 100));
        assert_eq!(transfer.source(), Some([1; 8]));

        let entry = Action::Entry(EntryAction::new([3; 8]));
        assert_eq!(entry.source(), None);
    }

    #[test]
    fn test_exit_action_size() {
        // 32 (source) + 40 (spk words) + 8 (amount) = 80 bytes = 20 words
        assert_eq!(ExitAction::SIZE, 80);
        assert_eq!(ExitAction::WORDS, 20);
    }

    #[test]
    fn test_exit_payload_size() {
        // 8 + 80 = 88 bytes = 22 words
        assert_eq!(EXIT_PAYLOAD_SIZE, 88);
        assert_eq!(EXIT_PAYLOAD_WORDS, 22);
    }

    #[test]
    fn test_exit_valid() {
        let p2pk_spk = crate::pay_to_pubkey_spk(&[0x42; 32]);
        let exit = ExitAction::new([1; 8], &p2pk_spk, 100);
        assert!(exit.is_valid());

        let zero_amount = ExitAction::new([1; 8], &p2pk_spk, 0);
        assert!(!zero_amount.is_valid());
    }

    #[test]
    fn test_exit_spk_p2pk_schnorr() {
        let pubkey = [0x42u8; 32];
        let p2pk_spk = crate::pay_to_pubkey_spk(&pubkey);
        assert_eq!(p2pk_spk.len(), 34);

        let exit = ExitAction::new([1; 8], &p2pk_spk, 100);
        assert_eq!(exit.spk_len(), 34);
        assert_eq!(exit.destination_spk_bytes(), &p2pk_spk);
    }

    #[test]
    fn test_exit_spk_p2pk_ecdsa() {
        // ECDSA P2PK: OP_DATA_33 (0x21) || 33-byte compressed pubkey || OP_CHECK_SIG_ECDSA (0xab)
        let compressed_pubkey = [0x02u8; 33]; // mock compressed pubkey
        let mut ecdsa_spk = [0u8; 35];
        ecdsa_spk[0] = 0x21; // OP_DATA_33
        ecdsa_spk[1..34].copy_from_slice(&compressed_pubkey);
        ecdsa_spk[34] = 0xab; // OP_CHECK_SIG_ECDSA

        let exit = ExitAction::new([1; 8], &ecdsa_spk, 300);
        assert_eq!(exit.spk_len(), 35);
        assert_eq!(exit.destination_spk_bytes(), &ecdsa_spk);
    }

    #[test]
    fn test_exit_spk_p2pk_ecdsa_matches_kaspa() {
        use kaspa_addresses::{Address, Prefix, Version};
        let compressed_pubkey = [0x02u8; 33];
        let addr = Address::new(Prefix::Mainnet, Version::PubKeyECDSA, &compressed_pubkey);
        let kaspa_spk = kaspa_txscript::pay_to_address_script(&addr);

        let exit = ExitAction::new([1; 8], kaspa_spk.script(), 300);
        assert_eq!(exit.spk_len(), 35);
        assert_eq!(exit.destination_spk_bytes(), kaspa_spk.script());
    }

    #[test]
    fn test_exit_spk_p2sh() {
        let script_hash = [0xAB; 32];
        let p2sh_spk = crate::pay_to_script_hash_spk(&script_hash);
        assert_eq!(p2sh_spk.len(), 35);

        let exit = ExitAction::new([1; 8], &p2sh_spk, 200);
        assert_eq!(exit.spk_len(), 35);
        assert_eq!(exit.destination_spk_bytes(), &p2sh_spk);
    }

    #[test]
    fn test_exit_roundtrip() {
        let p2pk_spk = crate::pay_to_pubkey_spk(&[0x42; 32]);
        let exit = ExitAction::new([0xAAAAAAAA; 8], &p2pk_spk, 999);
        let words: [u32; ExitAction::WORDS] = bytemuck::cast(exit);
        let restored = ExitAction::from_words(words);
        assert_eq!(exit, restored);
        assert_eq!(restored.destination_spk_bytes(), &p2pk_spk);
    }

    #[test]
    fn test_exit_roundtrip_ecdsa() {
        let mut ecdsa_spk = [0u8; 35];
        ecdsa_spk[0] = 0x21;
        ecdsa_spk[1..34].copy_from_slice(&[0x03; 33]);
        ecdsa_spk[34] = 0xab;

        let exit = ExitAction::new([0xBBBBBBBB; 8], &ecdsa_spk, 555);
        let words: [u32; ExitAction::WORDS] = bytemuck::cast(exit);
        let restored = ExitAction::from_words(words);
        assert_eq!(exit, restored);
        assert_eq!(restored.destination_spk_bytes(), &ecdsa_spk);
    }

    #[test]
    fn test_action_as_variants() {
        let transfer = Action::Transfer(TransferAction::new([1; 8], [2; 8], 100));
        assert!(transfer.as_transfer().is_some());
        assert!(transfer.as_entry().is_none());

        let entry = Action::Entry(EntryAction::new([3; 8]));
        assert!(entry.as_transfer().is_none());
        assert!(entry.as_entry().is_some());

        let p2pk_spk = crate::pay_to_pubkey_spk(&[0x42; 32]);
        let exit = Action::Exit(ExitAction::new([4; 8], &p2pk_spk, 50));
        assert!(exit.as_exit().is_some());
        assert!(exit.as_transfer().is_none());
        assert_eq!(exit.source(), Some([4; 8]));
    }
}
