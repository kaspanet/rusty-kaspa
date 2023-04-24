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
        RpcPeerInfo {
            id: peer.identity(),
            address: peer.net_address().into(),
            is_outbound: peer.is_outbound(),
            is_ibd_peer: ibd_peer_key.is_some() && peer.key() == *ibd_peer_key.as_ref().unwrap(),
            last_ping_duration: 0,          // TODO
            time_offset: 0,                 // TODO
            user_agent: Default::default(), // TODO
            advertised_protocol_version: 0, // TODO
            time_connected: 0,              // TODO
        }
    }

    pub fn get_peers_info(&self, peers: &[Peer]) -> Vec<RpcPeerInfo> {
        let ibd_peer_key = self.flow_context.ibd_peer();
        peers.iter().map(|x| self.get_peer_info(x, &ibd_peer_key)).collect()
    }
}
