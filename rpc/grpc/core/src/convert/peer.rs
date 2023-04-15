use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::{RpcError, RpcPeerAddress};

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcPeerInfo, protowire::GetConnectedPeerInfoMessage, {
    Self {
        id: item.id.to_string(),               // TODO
        address: item.address.address.clone(), // TODO
        last_ping_duration: item.last_ping_duration as i64,
        is_outbound: item.is_outbound,
        time_offset: item.time_offset as i64,
        user_agent: item.user_agent.clone(),
        advertised_protocol_version: item.advertised_protocol_version,
        time_connected: item.time_connected as i64,
        is_ibd_peer: item.is_ibd_peer,
    }
});

from!(item: &kaspa_rpc_core::RpcPeerAddress, protowire::GetPeerAddressesKnownAddressMessage, { Self { addr: item.address.clone() } });

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::GetConnectedPeerInfoMessage, kaspa_rpc_core::RpcPeerInfo, {
    Self {
        id: 0, // TODO
        address: RpcPeerAddress { address: item.address.clone() },
        last_ping_duration: item.last_ping_duration as u64,
        is_outbound: item.is_outbound,
        time_offset: item.time_offset as u64,
        user_agent: item.user_agent.clone(),
        advertised_protocol_version: item.advertised_protocol_version,
        time_connected: item.time_connected as u64,
        is_ibd_peer: item.is_ibd_peer,
    }
});

try_from!(item: &protowire::GetPeerAddressesKnownAddressMessage, kaspa_rpc_core::RpcPeerAddress, {
    Self { address: item.addr.clone() }
});
