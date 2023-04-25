use std::sync::Arc;

use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_p2p_lib::{Peer, PeerKey};
use kaspa_rpc_core::RpcPeerInfo;

pub struct ProtocolConverter {
    flow_context: Arc<FlowContext>,
}

impl ProtocolConverter {
    pub fn new(flow_context: Arc<FlowContext>) -> Self {
        Self { flow_context }
    }

    fn get_peer_info(&self, peer: &Peer, ibd_peer_key: &Option<PeerKey>) -> RpcPeerInfo {
        let properties = peer.properties();
        RpcPeerInfo {
            id: peer.identity(),
            address: peer.net_address().into(),
            is_outbound: peer.is_outbound(),
            is_ibd_peer: ibd_peer_key.is_some() && peer.key() == *ibd_peer_key.as_ref().unwrap(),
            last_ping_duration: peer.last_ping_duration(),
            time_offset: properties.time_offset,
            user_agent: properties.user_agent.clone(),
            advertised_protocol_version: properties.advertised_protocol_version,
            time_connected: peer.time_connected(),
        }
    }

    pub fn get_peers_info(&self, peers: &[Peer]) -> Vec<RpcPeerInfo> {
        let ibd_peer_key = self.flow_context.ibd_peer_key();
        peers.iter().map(|x| self.get_peer_info(x, &ibd_peer_key)).collect()
    }
}
