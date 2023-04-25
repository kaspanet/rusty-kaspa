use std::str::FromStr;

use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::{RpcError, RpcNodeId, RpcPeerAddress};

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcPeerInfo, protowire::GetConnectedPeerInfoMessage, {
    Self {
        id: item.id.to_string(),
        address: item.address.to_string(),
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
