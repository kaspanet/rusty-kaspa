extern crate derive_more;

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use consensus_core::subnets::{SubnetworkId, SUBNETWORK_ID_SIZE};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display, Formatter};
use std::str::{self, FromStr};

use crate::RpcError;

/// The Rpc version of a domain representation of a Subnetwork ID
///
/// ### Implementation notes
///
/// The representation is duplicate of the matching consensus-core [`SubnetworkId`]
/// and not a newtype because [`BorshSchema`] will not accept a field that does not
/// itself implement the trait.
///
/// TODO: Investigate if we really need the rpc-core structs to implement [`BorshSchema`].
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase", try_from = "String", into = "String")]
pub struct RpcSubnetworkId([u8; SUBNETWORK_ID_SIZE]);

impl AsRef<[u8]> for RpcSubnetworkId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl RpcSubnetworkId {
    pub const fn from_byte(b: u8) -> RpcSubnetworkId {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        bytes[0] = b;
        RpcSubnetworkId(bytes)
    }

    pub const fn from_bytes(bytes: [u8; SUBNETWORK_ID_SIZE]) -> RpcSubnetworkId {
        RpcSubnetworkId(bytes)
    }
}

impl From<SubnetworkId> for RpcSubnetworkId {
    fn from(item: SubnetworkId) -> Self {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        bytes.copy_from_slice(item.as_ref());
        RpcSubnetworkId(bytes)
    }
}

impl From<RpcSubnetworkId> for SubnetworkId {
    fn from(item: RpcSubnetworkId) -> Self {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        bytes.copy_from_slice(item.as_ref());
        SubnetworkId::from_bytes(bytes)
    }
}

impl Display for RpcSubnetworkId {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut hex = [0u8; SUBNETWORK_ID_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("The output is exactly twice the size of the input");
        f.write_str(str::from_utf8(&hex).expect("hex is always valid UTF-8"))
    }
}

impl From<RpcSubnetworkId> for String {
    fn from(item: RpcSubnetworkId) -> String {
        item.to_string()
    }
}

impl FromStr for RpcSubnetworkId {
    type Err = RpcError;

    #[inline]
    fn from_str(str: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        faster_hex::hex_decode(str.as_bytes(), &mut bytes)?;
        Ok(RpcSubnetworkId(bytes))
    }
}

impl TryFrom<&str> for RpcSubnetworkId {
    type Error = RpcError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for RpcSubnetworkId {
    type Error = RpcError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
