use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct RpcPeerAddress {
    // FIXME type
    pub address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct RpcPeerInfo {
    // FIXME type
    id: u64,
    // FIXME type
    address: RpcPeerAddress,

    // NOTE: gRPC proto has below u64 values as i64
    // should they be i64 here? (using u64 avoids conversion
    // when using trait, offloading it on gRPC)
    last_ping_duration: u64,
    is_outbound: bool,
    time_offset: u64,
    user_agent: String,
    advertised_protocol_version: u32,
    time_connected: u64,
    id_ibd_peer: bool,
}
