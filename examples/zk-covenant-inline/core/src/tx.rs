use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};
use core::array::TryFromSliceError;
use core::fmt::{Debug, Formatter};
use smallvec::SmallVec;

pub mod hashing;

/// Represents a Kaspa transaction
#[derive(Debug, Clone, PartialEq, Eq, Default, BorshSerialize, BorshDeserialize)]
pub struct Transaction {
    pub version: u16,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub lock_time: u64,
    pub subnetwork_id: SubnetworkId,
    pub gas: u64,
    pub payload: Vec<u8>,
}

/// Represents a Kaspa transaction input
#[derive(Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TransactionInput {
    pub previous_outpoint: TransactionOutpoint,
    pub signature_script: Vec<u8>, // TODO: Consider using SmallVec
    pub sequence: u64,

    // TODO: Since this field is used for calculating mass context free, and we already commit
    // to the mass in a dedicated field (on the tx level), it follows that this field is no longer
    // needed, and can be removed if we ever implement a v2 transaction
    pub sig_op_count: u8,
}
impl TransactionInput {
    pub fn new(previous_outpoint: TransactionOutpoint, signature_script: Vec<u8>, sequence: u64, sig_op_count: u8) -> Self {
        Self { previous_outpoint, signature_script, sequence, sig_op_count }
    }
}

impl core::fmt::Debug for TransactionInput {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TransactionInput")
            .field("previous_outpoint", &self.previous_outpoint)
            .field("signature_script", &"todo")
            .field("sequence", &self.sequence)
            .field("sig_op_count", &self.sig_op_count)
            .finish()
    }
}

/// Represents a Kaspad transaction output
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TransactionOutput {
    pub value: u64,
    pub script_public_key: ScriptPublicKey,
}

impl TransactionOutput {
    pub fn new(value: u64, script_public_key: ScriptPublicKey) -> Self {
        Self { value, script_public_key }
    }
}

/// Size of the underlying script vector of a script.
pub const SCRIPT_VECTOR_SIZE: usize = 35;

/// Used as the underlying type for script public key data, optimized for the common p2pk script size (34).
pub type ScriptVec = SmallVec<[u8; SCRIPT_VECTOR_SIZE]>;

/// Represents a Kaspad ScriptPublicKey
/// @category Consensus
#[derive(Default, PartialEq, Eq, Clone, Hash)]
pub struct ScriptPublicKey {
    pub version: ScriptPublicKeyVersion,
    pub(super) script: ScriptVec, // Kept private to preserve read-only semantics
}

/// Represents the ScriptPublicKey Version
pub type ScriptPublicKeyVersion = u16;

impl ScriptPublicKey {
    pub fn new(version: ScriptPublicKeyVersion, script: ScriptVec) -> Self {
        Self { version, script }
    }

    pub fn from_vec(version: ScriptPublicKeyVersion, script: Vec<u8>) -> Self {
        Self { version, script: ScriptVec::from_vec(script) }
    }

    pub fn version(&self) -> ScriptPublicKeyVersion {
        self.version
    }

    pub fn script(&self) -> &[u8] {
        &self.script
    }
}

impl core::fmt::Debug for ScriptPublicKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ScriptPublicKey").field("version", &self.version).field("script", &"todo").finish()
    }
}

impl BorshSerialize for ScriptPublicKey {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.version, writer)?;
        // Vectors and slices are all serialized internally the same way
        borsh::BorshSerialize::serialize(&self.script.as_slice(), writer)?;
        Ok(())
    }
}

impl BorshDeserialize for ScriptPublicKey {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        // Deserialize into vec first since we have no custom smallvec support
        Ok(Self::from_vec(borsh::BorshDeserialize::deserialize_reader(reader)?, borsh::BorshDeserialize::deserialize_reader(reader)?))
    }
}

/// The size of the array used to store subnetwork IDs.
pub const SUBNETWORK_ID_SIZE: usize = 20;

/// The domain representation of a Subnetwork ID
#[derive(Clone, Default, Eq, PartialEq, Ord, PartialOrd, Hash, BorshSerialize, BorshDeserialize, Copy)]
pub struct SubnetworkId([u8; SUBNETWORK_ID_SIZE]);

impl Debug for SubnetworkId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SubnetworkId").field("", &"todo").finish()
    }
}
const HASH_SIZE: usize = 32;

#[derive(Eq, Clone, Copy, Default, PartialOrd, Ord, BorshSerialize, BorshDeserialize, PartialEq, Hash, Debug)]
#[repr(transparent)]
pub struct TransactionId([u8; 32]);

impl From<[u8; HASH_SIZE]> for TransactionId {
    fn from(value: [u8; HASH_SIZE]) -> Self {
        TransactionId(value)
    }
}

impl TryFrom<&[u8]> for TransactionId {
    type Error = TryFromSliceError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        TransactionId::try_from_slice(value)
    }
}

impl TransactionId {
    #[inline(always)]
    pub const fn from_bytes(bytes: [u8; HASH_SIZE]) -> Self {
        TransactionId(bytes)
    }

    #[inline(always)]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[inline(always)]
    pub const fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    #[inline(always)]
    /// # Panics
    /// Panics if `bytes` length is not exactly `HASH_SIZE`.
    pub fn from_slice(bytes: &[u8]) -> Self {
        Self(<[u8; HASH_SIZE]>::try_from(bytes).expect("Slice must have the length of Hash"))
    }

    #[inline(always)]
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, TryFromSliceError> {
        Ok(Self(<[u8; HASH_SIZE]>::try_from(bytes)?))
    }

    #[inline(always)]
    pub fn to_le_u64(self) -> [u64; 4] {
        let mut out = [0u64; 4];
        out.iter_mut().zip(self.iter_le_u64()).for_each(|(out, word)| *out = word);
        out
    }

    #[inline(always)]
    pub fn iter_le_u64(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
        self.0.chunks_exact(8).map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
    }

    #[inline(always)]
    pub fn from_le_u64(arr: [u64; 4]) -> Self {
        let mut ret = [0; HASH_SIZE];
        ret.chunks_exact_mut(8).zip(arr.iter()).for_each(|(bytes, word)| bytes.copy_from_slice(&word.to_le_bytes()));
        Self(ret)
    }

    #[inline(always)]
    pub fn from_u64_word(word: u64) -> Self {
        Self::from_le_u64([0, 0, 0, word])
    }
}

#[derive(Eq, Default, Hash, PartialEq, Debug, Copy, Clone, BorshSerialize, BorshDeserialize)]
pub struct TransactionOutpoint {
    pub transaction_id: TransactionId,
    pub index: TransactionIndexType,
}
pub type TransactionIndexType = u32;
