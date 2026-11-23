use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_utils::{
    connection::Libp2pId,
    networking::{ContextualNetAddress, IpAddress, NetAddress, PeerId},
};
use serde::{Deserialize, Serialize};

pub type RpcNodeId = PeerId;
pub type RpcIpAddress = IpAddress;
pub type RpcPeerAddress = NetAddress;
pub type RpcContextualPeerAddress = ContextualNetAddress;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RpcPeerInfo {
    pub id: RpcNodeId,
    pub address: RpcPeerAddress,
    /// Authenticated libp2p identity; absent for TCP connections.
    pub libp2p_id: Option<Libp2pId>,
    pub last_ping_duration: u64, // NOTE: i64 in gRPC protowire

    pub is_outbound: bool,
    pub time_offset: i64,
    pub user_agent: String,

    pub advertised_protocol_version: u32,
    pub time_connected: u64, // NOTE: i64 in gRPC protowire
    pub is_ibd_peer: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GetConnectedPeerInfoResponse;

    fn peer(address: &str, libp2p_id: Option<Libp2pId>) -> RpcPeerInfo {
        RpcPeerInfo {
            id: RpcNodeId::default(),
            address: address.parse().unwrap(),
            libp2p_id,
            last_ping_duration: 42,
            is_outbound: true,
            time_offset: -1,
            user_agent: "/kaspa:test/".into(),
            advertised_protocol_version: 11,
            time_connected: 100,
            is_ibd_peer: false,
        }
    }

    #[test]
    fn connected_peer_socket_and_optional_identity_round_trip() {
        for address in ["1.2.3.4:16111", "[2001:4860::1]:16512"] {
            for id in [None, Some([7; 32])] {
                let peer = peer(address, id);
                let json = serde_json::to_value(&peer).unwrap();
                assert_eq!(json["address"], serde_json::to_value(peer.address).unwrap());
                let decoded: RpcPeerInfo = serde_json::from_value(json.clone()).unwrap();
                assert_eq!(decoded.address, peer.address);
                assert_eq!(decoded.libp2p_id, id);
                let decoded = borsh::from_slice::<RpcPeerInfo>(&borsh::to_vec(&peer).unwrap()).unwrap();
                assert_eq!(serde_json::to_value(decoded).unwrap(), json);
                let mut legacy_json = json;
                legacy_json.as_object_mut().unwrap().remove("libp2p_id");
                assert!(serde_json::from_value::<RpcPeerInfo>(legacy_json).unwrap().libp2p_id.is_none());
            }
        }
    }

    #[test]
    fn version_one_connected_peer_response_decodes_without_identity() {
        let peers = [peer("1.2.3.4:16111", None), peer("[2001:4860::1]:16512", None)];
        // Tuples have the same Borsh layout as master's RpcPeerInfo, which has no libp2p ID.
        let legacy = peers
            .iter()
            .map(|peer| {
                (
                    peer.id,
                    peer.address,
                    peer.last_ping_duration,
                    peer.is_outbound,
                    peer.time_offset,
                    &peer.user_agent,
                    peer.advertised_protocol_version,
                    peer.time_connected,
                    peer.is_ibd_peer,
                )
            })
            .collect::<Vec<_>>();
        let mut bytes = borsh::to_vec(&1u16).unwrap();
        BorshSerialize::serialize(&legacy, &mut bytes).unwrap();
        BorshSerialize::serialize(&0x12345678u32, &mut bytes).unwrap();
        let mut reader = bytes.as_slice();
        let decoded: GetConnectedPeerInfoResponse = workflow_serializer::prelude::Deserializer::deserialize(&mut reader).unwrap();
        assert_eq!(serde_json::to_value(decoded.peer_info).unwrap(), serde_json::to_value(peers).unwrap());
        assert_eq!(u32::deserialize_reader(&mut reader).unwrap(), 0x12345678);
        assert!(reader.is_empty());
    }
}
