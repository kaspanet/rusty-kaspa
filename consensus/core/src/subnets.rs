use std::fmt::{Debug, Display, Formatter};
use std::str::{self, FromStr};

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_utils::hex::{FromHex, ToHex};
use kaspa_utils::{serde_impl_deser_fixed_bytes_ref, serde_impl_ser_fixed_bytes_ref};
use thiserror::Error;

/// The size of the array used to store subnetwork IDs.
pub const SUBNETWORK_ID_SIZE: usize = 20;

/// The domain representation of a Subnetwork ID
#[derive(Clone, Default, Eq, PartialEq, Ord, PartialOrd, Hash, BorshSerialize, BorshDeserialize)]
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

impl SubnetworkId {
    pub const fn from_byte(b: u8) -> SubnetworkId {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        bytes[0] = b;
        SubnetworkId(bytes)
    }

    pub const fn from_bytes(bytes: [u8; SUBNETWORK_ID_SIZE]) -> SubnetworkId {
        SubnetworkId(bytes)
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
pub const SUBNETWORK_ID_NATIVE: SubnetworkId = SubnetworkId::from_byte(0);

/// The subnetwork ID which is used for the coinbase transaction
pub const SUBNETWORK_ID_COINBASE: SubnetworkId = SubnetworkId::from_byte(1);

/// The subnetwork ID which is used for adding new sub networks to the registry
pub const SUBNETWORK_ID_REGISTRY: SubnetworkId = SubnetworkId::from_byte(2);
