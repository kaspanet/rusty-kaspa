use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_utils::networking::{ContextualNetAddress, IpAddress, NetAddress, PeerEndpoint, PeerId};
use serde::{Deserialize, Serialize};

pub type RpcNodeId = PeerId;
pub type RpcIpAddress = IpAddress;
pub type RpcPeerAddress = NetAddress;
pub type RpcContextualPeerAddress = ContextualNetAddress;
/// Operator-facing peer endpoint accepted by the RPC `AddPeer` method
/// and the `--addpeer` / `--connect` CLI flags. Wraps either a numeric
/// IP literal or a textual hostname; the connection manager resolves
/// hostnames at dial time.
pub type RpcPeerEndpoint = PeerEndpoint;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RpcPeerInfo {
    pub id: RpcNodeId,
    pub address: RpcPeerAddress,
    pub last_ping_duration: u64, // NOTE: i64 in gRPC protowire

    pub is_outbound: bool,
    pub time_offset: i64,
    pub user_agent: String,

    pub advertised_protocol_version: u32,
    pub time_connected: u64, // NOTE: i64 in gRPC protowire
    pub is_ibd_peer: bool,
}
