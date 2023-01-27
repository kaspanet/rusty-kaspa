use crate::kaspa_flows;
use crate::kaspa_flows::FlowRegistryApi;
use crate::kaspa_grpc;
use crate::kaspa_grpc::RouterApi;
use crate::pb;
use kaspa_core::{debug, error};
use log::warn;
use std::sync::Arc;
use tonic::async_trait;

#[allow(dead_code)]
type P2pClientType = kaspa_grpc::P2pClient<kaspa_grpc::Router>;

#[async_trait]
pub trait P2pAdaptorApi {
    // will be used only for client side connections (regular kaspa node will NOT use it)
    async fn init_only_client_side() -> Option<std::sync::Arc<Self>>;
    // will start new grpc listener + all infra needed
    // 1) start listener + grpc
    // 2) start new flows registration loop
    // 3) register flows terminate channels
    async fn listen(ip_port: String) -> Option<std::sync::Arc<Self>>;
    // will start new client connection
    async fn connect_peer(&self, ip_port: String) -> Option<uuid::Uuid>;
    // send message to peer - used for tests (regular kaspa node will NOT use it)
    async fn send(&self, id: uuid::Uuid, msg: pb::KaspadMessage);
    // async fn send2<T: ToPayload>(&self, id: uuid::Uuid, msg: T);
    // will terminate everything, but p2p layer
    // p2p layer will be terminated during drop(...)
    async fn terminate(&self, id: uuid::Uuid);
    async fn terminate_all_peers_and_flows(&self);
    // helper functions
    fn get_all_peer_ids(&self) -> std::vec::Vec<uuid::Uuid>;
    fn get_all_flow_ids(&self) -> std::vec::Vec<uuid::Uuid>;
}

#[allow(dead_code)]
pub struct P2pAdaptor {
    master_router: std::sync::Arc<kaspa_grpc::Router>,
    flow_termination: lockfree::map::Map<uuid::Uuid, kaspa_flows::FlowTxTerminateChannelType>,
    p2p_termination: Option<tokio::sync::oneshot::Sender<()>>,
    peers: lockfree::map::Map<uuid::Uuid, kaspa_grpc::P2pClient<kaspa_grpc::Router>>,
}

/*
pub trait ToPayload {
    fn to_payload(self) -> pb::kaspad_message::Payload;
}
#[macro_export]
macro_rules! to_payload {
    ($message:ident, $payload:ident) => {
        impl ToPayload for $message {
            fn to_payload(self) -> pb::kaspad_message::Payload {
                pb::kaspad_message::Payload::$payload(pb::$message)
            }
        }
    };
}

to_payload! { VerackMessage, Verack }
*/
#[async_trait]
impl P2pAdaptorApi for P2pAdaptor {
    async fn init_only_client_side() -> Option<Arc<Self>> {
        // [0] - Create new router - first instance
        // upper_layer_rx will be used to dispatch notifications about new-connections, both for client & server
        let (master_router, mut upper_layer_rx) = kaspa_grpc::Router::new().await;
        // [1] - Create adaptor
        let p2p_adaptor = std::sync::Arc::new(P2pAdaptor {
            master_router,
            flow_termination: lockfree::map::Map::new(),
            p2p_termination: None,
            peers: lockfree::map::Map::new(),
        });
        let p2p_adaptor_clone = p2p_adaptor.clone();
        // [2] - Start service layer to listen when new connection is coming ( Server & Client side )
        tokio::spawn(async move {
            // loop will exit when all sender channels will be dropped
            // --> when all routers will be dropped & grpc-service will be stopped
            while let Some(new_router) = upper_layer_rx.recv().await {
                // as en example subscribe to all message-types, in reality different flows will subscribe to different message-types
                let new_router_id = new_router.identity();
                let flow_terminate = kaspa_flows::FlowRegistry::initialize_flow(new_router).await;
                let result = p2p_adaptor.flow_termination.insert(new_router_id, flow_terminate);
                if result.is_some() {
                    panic!(
                        "At flow initialization, insertion into the map - got existing value, flow-key = router-id: {:?}",
                        result.unwrap().key()
                    );
                }
            }
        });
        Some(p2p_adaptor_clone)
    }
    // regular kaspa node will use this call to have both server & client connections
    async fn listen(ip_port: String) -> Option<std::sync::Arc<Self>> {
        // [0] - Create new router - first instance
        // upper_layer_rx will be used to dispatch notifications about new-connections, both for client & server
        let (master_router, mut upper_layer_rx) = kaspa_grpc::Router::new().await;
        // [1] - Start listener (de-facto Server side )
        let terminate_server = kaspa_grpc::P2pServer::listen(ip_port, master_router.clone(), true).await;
        // [2] - Check that server is ok & register termination signal ( as an example )
        if let Ok(t) = terminate_server {
            debug!("P2P, Server is running ...");
            let p2p_adaptor = std::sync::Arc::new(P2pAdaptor {
                master_router,
                flow_termination: lockfree::map::Map::new(),
                p2p_termination: Some(t),
                peers: lockfree::map::Map::new(),
            });
            let p2p_adaptor_clone = p2p_adaptor.clone();
            // [3] - Start service layer to listen when new connection is coming ( Server & Client side )
            tokio::spawn(async move {
                // loop will exit when all sender channels will be dropped
                // --> when all routers will be dropped & grpc-service will be stopped
                while let Some(new_router) = upper_layer_rx.recv().await {
                    // as en example subscribe to all message-types, in reality different flows will subscribe to different message-types
                    let new_router_id = new_router.identity();
                    let flow_terminate = kaspa_flows::FlowRegistry::initialize_flow(new_router).await;
                    let result = p2p_adaptor.flow_termination.insert(new_router_id, flow_terminate);
                    if result.is_some() {
                        panic!(
                            "At flow initialization, insertion into the map - got existing value, flow-key = router-id: {:?}",
                            result.unwrap().key()
                        );
                    }
                }
            });
            Some(p2p_adaptor_clone)
        } else {
            error!("P2P, Server can't start, {:?}", terminate_server.err());
            None
        }
    }

    async fn connect_peer(&self, ip_port: String) -> Option<uuid::Uuid> {
        // [0] - Start client + re-connect loop
        let client = kaspa_grpc::P2pClient::connect_with_retry(ip_port, self.master_router.clone(), false, 16).await;
        match client {
            Some(connected_client) => {
                let peer_id = connected_client.router.identity();
                self.peers.insert(peer_id, connected_client);
                Some(peer_id)
            }
            None => {
                debug!("P2P, Client connection failed - 16 retries ...");
                None
            }
        }
    }

    async fn send(&self, id: uuid::Uuid, msg: pb::KaspadMessage) {
        match self.peers.get(&id) {
            Some(p2p_client) => {
                let result = p2p_client.val().router.route_to_network(msg).await;
                if !result {
                    warn!("P2P, P2PAdaptor::send<T> - can't route message to peer-id: {:?}", id);
                }
            }
            None => {
                warn!("P2P, P2PAdaptor::send<T> - try to send message to peer that does not exist, peer-id: {:?}", id);
            }
        }
    }

    /*
    async fn send2<T: ToPayload>(&self, id: uuid::Uuid, msg: T) {
        self.send(id, KaspadMessage { payload: Some(msg.to_payload()) }).await;
    }

     */

    async fn terminate(&self, id: uuid::Uuid) {
        match self.peers.remove(&id) {
            Some(peer) => {
                peer.val().router.close().await;
                debug!("P2P, P2pAdaptor::terminate - peer-id: {:?}, is terminated", id);
            }
            None => {
                warn!("P2P, P2pAdaptor::terminate - try to remove unknown peer-id: {:?}", id);
            }
        }
        match self.flow_termination.remove(&id) {
            Some(_flow_terminate_channel) => {
                //let _ = flow_terminate_channel.val().send(());
                debug!("P2P, P2pAdaptor::terminate - flow-id: {:?}, is terminated", id);
            }
            None => {
                warn!("P2P, P2pAdaptor::terminate - try to remove unknown flow-id: {:?}", id);
            }
        }
    }

    async fn terminate_all_peers_and_flows(&self) {
        let peer_ids = self.get_all_peer_ids();
        for peer_id in peer_ids.iter() {
            match self.peers.remove(peer_id) {
                Some(peer) => {
                    peer.val().router.close().await;
                    debug!("P2P, P2pAdaptor::terminate_all_peers_and_flows - peer-id: {:?}, is terminated", peer_id);
                }
                None => {
                    warn!("P2P, P2pAdaptor::terminate_all_peers_and_flows - try to remove unknown peer-id: {:?}", peer_id);
                }
            }
        }
        let flow_ids = self.get_all_flow_ids();
        for flow_id in flow_ids.iter() {
            match self.flow_termination.remove(flow_id) {
                Some(_flow_terminate_channel) => {
                    debug!("P2P, P2pAdaptor::terminate_all_peers_and_flows - flow-id: {:?}, is terminated", flow_id);
                }
                None => {
                    warn!("P2P, P2pAdaptor::terminate_all_peers_and_flows - try to remove unknown flow-id: {:?}", flow_id);
                }
            }
        }
        // commented but maybe used later
        /*
        if false == peer_ids.eq(&flow_ids) {
            warn!("P2P, P2pAdaptor::terminate_all_peers_and_flows - peers-ids are not equal to flow_ids");
            trace!("P2P, P2pAdaptor::terminate_all_peers_and_flows - peer-ids: {:?}", peer_ids);
            trace!("P2P, P2pAdaptor::terminate_all_peers_and_flows - flow-ids: {:?}", flow_ids);
        }
         */
    }

    fn get_all_peer_ids(&self) -> Vec<uuid::Uuid> {
        let mut ids = std::vec::Vec::<uuid::Uuid>::new();
        for peer in self.peers.iter() {
            ids.push(*peer.key());
        }
        /*
        let mut it = self.peers.iter();
        loop {
            match it.next() {
                Some(v) => {
                    ids.push(v.key().clone());
                }
                None => break,
            }
        }

         */
        ids
    }
    fn get_all_flow_ids(&self) -> Vec<uuid::Uuid> {
        let mut ids = std::vec::Vec::<uuid::Uuid>::new();
        for flow in self.flow_termination.iter() {
            ids.push(*flow.key());
        }
        ids
    }
}
