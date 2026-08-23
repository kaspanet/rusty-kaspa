use std::fmt::{Debug, Display, Formatter};
use std::str::{self, FromStr};

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_utils::hex::{FromHex, ToHex};
use kaspa_utils::{serde_impl_deser_fixed_bytes_ref, serde_impl_ser_fixed_bytes_ref};
use thiserror::Error;

/// The size of the array used to store subnetwork IDs.
pub const SUBNETWORK_ID_SIZE: usize = 20;

/// Length of the user-lane namespace prefix. Per KIP-21, a user-lane subnetwork
/// ID has the shape `[namespace (4 bytes), zero tail (16 bytes)]` with at least
/// one non-zero byte in the namespace. Reserved IDs (`[x, 0×19]`) are handled
/// separately and constrained to [`NativeSubnetwork::FIRST_BYTE`] /
/// [`CoinbaseSubnetwork::FIRST_BYTE`].
pub const SUBNETWORK_NAMESPACE_LEN: usize = 4;

/// Number of trailing zero bytes required in a user-lane subnetwork ID.
pub const SUBNETWORK_ZERO_TAIL_LEN: usize = SUBNETWORK_ID_SIZE - SUBNETWORK_NAMESPACE_LEN;

const _: () = assert!(SUBNETWORK_NAMESPACE_LEN + SUBNETWORK_ZERO_TAIL_LEN == SUBNETWORK_ID_SIZE);

/// The domain representation of a Subnetwork ID
#[derive(Clone, Default, Eq, PartialEq, Ord, PartialOrd, Hash, BorshSerialize, BorshDeserialize, Copy)]
#[repr(transparent)]
pub struct SubnetworkId([u8; SUBNETWORK_ID_SIZE]);

impl Debug for SubnetworkId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubnetworkId").field("", &self.to_hex()).finish()
    }
}

serde_impl_ser_fixed_bytes_ref!(SubnetworkId, SUBNETWORK_ID_SIZE);
serde_impl_deser_fixed_bytes_ref!(SubnetworkId, SUBNETWORK_ID_SIZE);

impl AsRef<[u8; SUBNETWORK_ID_SIZE]> for SubnetworkId {
    fn as_ref(&self) -> &[u8; SUBNETWORK_ID_SIZE] {
        &self.0
    }
}

impl AsRef<[u8]> for SubnetworkId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; SUBNETWORK_ID_SIZE]> for SubnetworkId {
    fn from(value: [u8; SUBNETWORK_ID_SIZE]) -> Self {
        Self::from_bytes(value)
    }
}

impl From<SubnetworkId> for Vec<u8> {
    fn from(id: SubnetworkId) -> Self {
        id.0.into()
    }
}

impl SubnetworkId {
    pub const fn from_byte(b: u8) -> SubnetworkId {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        bytes[0] = b;
        SubnetworkId(bytes)
    }

    /// Construct a user-lane subnetwork ID with shape `[namespace, 0×16]`.
    /// The 4-byte namespace must have at least one non-zero byte — an all-zero
    /// namespace yields the native reserved ID. Validation enforces the shape
    /// at the consensus layer (see `check_transaction_subnetwork`).
    pub const fn from_namespace(namespace: [u8; SUBNETWORK_NAMESPACE_LEN]) -> SubnetworkId {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        let mut i = 0;
        while i < SUBNETWORK_NAMESPACE_LEN {
            bytes[i] = namespace[i];
            i += 1;
        }
        SubnetworkId(bytes)
    }

    pub const fn from_bytes(bytes: [u8; SUBNETWORK_ID_SIZE]) -> SubnetworkId {
        SubnetworkId(bytes)
    }

    pub const fn into_bytes(self) -> [u8; SUBNETWORK_ID_SIZE] {
        self.0
    }

    pub const fn as_bytes(&self) -> &[u8; SUBNETWORK_ID_SIZE] {
        &self.0
    }

    /// Returns true if the subnetwork is a built-in subnetwork, which
    /// means all nodes, including partial nodes, must validate it, and its transactions
    /// always use 0 gas.
    #[inline]
    pub fn is_builtin(&self) -> bool {
        *self == SUBNETWORK_ID_COINBASE || *self == SUBNETWORK_ID_REGISTRY
    }

    /// Returns true if the subnetwork is the native subnetwork
    #[inline]
    pub fn is_native(&self) -> bool {
        *self == SUBNETWORK_ID_NATIVE
    }

    /// Returns true if the subnetwork is the native or a built-in subnetwork
    #[inline]
    pub fn is_builtin_or_native(&self) -> bool {
        self.is_native() || self.is_builtin()
    }
}

#[derive(Error, Debug, Clone)]
pub enum SubnetworkConversionError {
    #[error(transparent)]
    SliceError(#[from] std::array::TryFromSliceError),

    #[error(transparent)]
    HexError(#[from] faster_hex::Error),
}

impl TryFrom<&[u8]> for SubnetworkId {
    type Error = SubnetworkConversionError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes = <[u8; SUBNETWORK_ID_SIZE]>::try_from(value)?;
        Ok(Self(bytes))
    }
}

impl Display for SubnetworkId {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut hex = [0u8; SUBNETWORK_ID_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("The output is exactly twice the size of the input");
        f.write_str(str::from_utf8(&hex).expect("hex is always valid UTF-8"))
    }
}

impl ToHex for SubnetworkId {
    fn to_hex(&self) -> String {
        let mut hex = [0u8; SUBNETWORK_ID_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("The output is exactly twice the size of the input");
        str::from_utf8(&hex).expect("hex is always valid UTF-8").to_string()
    }
}

impl FromStr for SubnetworkId {
    type Err = SubnetworkConversionError;

    #[inline]
    fn from_str(hex_str: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        faster_hex::hex_decode(hex_str.as_bytes(), &mut bytes)?;
        Ok(Self(bytes))
    }
}

impl FromHex for SubnetworkId {
    type Error = SubnetworkConversionError;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        faster_hex::hex_decode(hex_str.as_bytes(), &mut bytes)?;
        Ok(Self(bytes))
    }
}

/// The default subnetwork ID which is used for transactions without related payload data
pub const SUBNETWORK_ID_NATIVE: SubnetworkId = NativeSubnetwork::SUBNETWORK_ID;

/// The subnetwork ID which is used for the coinbase transaction
pub const SUBNETWORK_ID_COINBASE: SubnetworkId = CoinbaseSubnetwork::SUBNETWORK_ID;

/// The subnetwork ID which is used for adding new sub networks to the registry
pub const SUBNETWORK_ID_REGISTRY: SubnetworkId = RegistrySubnetwork::SUBNETWORK_ID;

/// Uninhabited marker types for reserved system subnetworks.
/// Per KIP-21, subnetwork IDs with a 19-byte zero suffix (`[x, 0×19]`) are reserved:
/// only [`NativeSubnetwork::FIRST_BYTE`] and [`CoinbaseSubnetwork::FIRST_BYTE`] are
/// valid. All other IDs must have the user-lane shape `[namespace (4 bytes), 0×16]`
/// with a non-zero namespace; any first byte is permitted in that form.
pub enum NativeSubnetwork {}

pub enum CoinbaseSubnetwork {}

pub enum RegistrySubnetwork {}

pub trait Subnetwork {
    const FIRST_BYTE: u8;
    const SUBNETWORK_ID: SubnetworkId = SubnetworkId::from_byte(Self::FIRST_BYTE);
}

impl Subnetwork for NativeSubnetwork {
    const FIRST_BYTE: u8 = 0;
}

impl Subnetwork for CoinbaseSubnetwork {
    const FIRST_BYTE: u8 = 1;
}

impl Subnetwork for RegistrySubnetwork {
    const FIRST_BYTE: u8 = 2;
}
