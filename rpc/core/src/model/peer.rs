use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use kaspa_addressmanager::NetAddress;
use serde::{Deserialize, Serialize};

pub type RpcPeerAddress = NetAddress;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct RpcPeerInfo {
    // TODO: fix the type (gRPC has a string)
    id: u64,
    address: RpcPeerAddress,
    last_ping_duration: u64, // NOTE: i64 in gRPC protowire

    is_outbound: bool,
    time_offset: u64,
    user_agent: String,

    advertised_protocol_version: u32,
    time_connected: u64, // NOTE: i64 in gRPC protowire
    is_ibd_peer: bool,
}
