use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use kaspa_addressmanager::{IpAddress, NetAddress};
use serde::{Deserialize, Serialize};

pub type RpcIpAddress = IpAddress;
pub type RpcPeerAddress = NetAddress;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct RpcPeerInfo {
    // TODO: fix the type (gRPC has a string)
    pub id: u64,
    pub address: RpcPeerAddress,
    pub last_ping_duration: u64, // NOTE: i64 in gRPC protowire

    pub is_outbound: bool,
    pub time_offset: u64,
    pub user_agent: String,

    pub advertised_protocol_version: u32,
    pub time_connected: u64, // NOTE: i64 in gRPC protowire
    pub is_ibd_peer: bool,
}
