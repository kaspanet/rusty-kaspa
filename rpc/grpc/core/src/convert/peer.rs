use std::str::FromStr;

use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::{RpcError, RpcNodeId, RpcPeerAddress};
use kaspa_utils::connection::Libp2pId;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcPeerInfo, protowire::GetConnectedPeerInfoMessage, {
    Self {
        id: item.id.to_string(),
        address: item.address.to_string(),
        libp2p_id: item.libp2p_id.map(|id| id.to_vec()),
        last_ping_duration: item.last_ping_duration as i64,
        is_outbound: item.is_outbound,
        time_offset: item.time_offset,
        user_agent: item.user_agent.clone(),
        advertised_protocol_version: item.advertised_protocol_version,
        time_connected: item.time_connected as i64,
        is_ibd_peer: item.is_ibd_peer,
    }
});

from!(item: &kaspa_rpc_core::RpcPeerAddress, protowire::GetPeerAddressesKnownAddressMessage, { Self { addr: item.to_string() } });
from!(item: &kaspa_rpc_core::RpcIpAddress, protowire::GetPeerAddressesKnownAddressMessage, { Self { addr: item.to_string() } });

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::GetConnectedPeerInfoMessage, kaspa_rpc_core::RpcPeerInfo, {
    Self {
        id: RpcNodeId::from_str(&item.id)?,
        address: RpcPeerAddress::from_str(&item.address)?,
        libp2p_id: item.libp2p_id.as_deref().map(|id| {
            id.try_into().map_err(|_| RpcError::General(format!("Libp2p ID must contain {} bytes, got {}", size_of::<Libp2pId>(), id.len())))
        }).transpose()?,
        last_ping_duration: item.last_ping_duration as u64,
        is_outbound: item.is_outbound,
        time_offset: item.time_offset,
        user_agent: item.user_agent.clone(),
        advertised_protocol_version: item.advertised_protocol_version,
        time_connected: item.time_connected as u64,
        is_ibd_peer: item.is_ibd_peer,
    }
});

try_from!(item: &protowire::GetPeerAddressesKnownAddressMessage, kaspa_rpc_core::RpcPeerAddress, { Self::from_str(&item.addr)? });
try_from!(item: &protowire::GetPeerAddressesKnownAddressMessage, kaspa_rpc_core::RpcIpAddress, { Self::from_str(&item.addr)? });

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_peer_keeps_socket_and_optional_libp2p_identity() {
        for address in ["1.2.3.4:16111", "[2001:4860::1]:16512"] {
            for libp2p_id in [None, Some(vec![7; 32])] {
                let wire = protowire::GetConnectedPeerInfoMessage {
                    id: RpcNodeId::default().to_string(),
                    address: address.into(),
                    libp2p_id: libp2p_id.clone(),
                    ..Default::default()
                };
                let peer = kaspa_rpc_core::RpcPeerInfo::try_from(&wire).unwrap();
                assert_eq!(peer.address.to_string(), address);
                assert_eq!(peer.libp2p_id.map(|id| id.to_vec()), libp2p_id);
                let encoded = protowire::GetConnectedPeerInfoMessage::from(&peer);
                assert_eq!(encoded.address, address);
                assert_eq!(encoded.libp2p_id, libp2p_id);
            }
        }
    }

    #[test]
    fn malformed_connected_peer_libp2p_identity_is_rejected() {
        for length in [0, 31, 33] {
            let wire = protowire::GetConnectedPeerInfoMessage {
                id: RpcNodeId::default().to_string(),
                address: "1.2.3.4:16111".into(),
                libp2p_id: Some(vec![7; length]),
                ..Default::default()
            };
            assert!(
                matches!(kaspa_rpc_core::RpcPeerInfo::try_from(&wire), Err(RpcError::General(message)) if message.contains("32 bytes"))
            );
        }
    }
}
