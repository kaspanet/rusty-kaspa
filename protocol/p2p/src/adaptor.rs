use crate::infra;
use crate::infra::P2pClient;
use crate::infra::P2pEvent;
use crate::infra::Router;
use crate::infra::RouterApi;
use crate::pb;
use crate::registry;
use crate::registry::FlowRegistryApi;
use crate::registry::P2pConnection;
use kaspa_core::warn;
use kaspa_core::{debug, error};
use lockfree::map::Map;
use std::sync::Arc;
use tonic::async_trait;

#[async_trait]
pub trait P2pAdaptorApi {
    /// Will be used only for client side connections (regular kaspa node will NOT use it)
    async fn init_only_client_side(flow_registry: Arc<dyn FlowRegistryApi>) -> Option<Arc<Self>>;

    /// Will start new grpc listener + all infra needed
    /// 1) start listener + grpc
    /// 2) start new flows registration loop
    /// 3) register flows terminate channels
    async fn listen(ip_port: String, flow_registry: Arc<dyn FlowRegistryApi>) -> Option<Arc<Self>>;

    /// Will start a new client connection
    async fn connect_peer(&self, ip_port: String) -> Option<uuid::Uuid>;

    /// Send message to peer - used for tests (regular kaspa node will NOT use it)
    async fn send(&self, id: uuid::Uuid, msg: pb::KaspadMessage);

    /// Terminate a specific peer/flow
    async fn terminate(&self, id: uuid::Uuid);

    /// Will terminate everything, but p2p layer
    /// p2p layer will be terminated during drop(...)
    async fn terminate_all_peers_and_flows(&self);

    /// Helper function to get all existing peer ids
    fn get_all_peer_ids(&self) -> std::vec::Vec<uuid::Uuid>;

    /// Helper function to get all outbound peer ids
    fn get_outbound_peer_ids(&self) -> Vec<uuid::Uuid>;

    /// Helper function to get all existing flow ids
    fn get_all_flow_ids(&self) -> std::vec::Vec<uuid::Uuid>;
}

#[allow(dead_code)]
pub struct P2pAdaptor {
    master_router: Arc<Router>,
    flow_termination: Map<uuid::Uuid, registry::FlowTxTerminateChannelType>,
    p2p_termination: Option<tokio::sync::oneshot::Sender<()>>,

    /// Holds router objects for all inbound/outbound peers
    peers: Map<uuid::Uuid, Arc<Router>>,

    /// Holds client objects only for outbound peers
    outbound_clients: Map<uuid::Uuid, P2pClient<Router>>,
}

#[async_trait]
impl P2pAdaptorApi for P2pAdaptor {
    async fn init_only_client_side(flow_registry: Arc<dyn FlowRegistryApi>) -> Option<Arc<Self>> {
        // [0] - Create new router - first instance
        // upper_layer_rx will be used to dispatch notifications about new-connections, both for client & server
        let (master_router, mut upper_layer_rx) = Router::new().await;
        // [1] - Create adaptor
        let p2p_adaptor = Arc::new(P2pAdaptor {
            master_router,
            flow_termination: Map::new(),
            p2p_termination: None,
            peers: Map::new(),
            outbound_clients: Map::new(),
        });
        let p2p_adaptor_clone = p2p_adaptor.clone();
        // [2] - Start service layer to listen when new connection is coming ( Server & Client side )
        tokio::spawn(async move {
            // loop will exit when all sender channels will be dropped
            // --> when all routers will be dropped & grpc-service will be stopped
            while let Some(new_event) = upper_layer_rx.recv().await {
                match new_event {
                    P2pEvent::NewRouter(new_router) => {
                        // Insert the new router to the adaptor peer list
                        let peer_id = new_router.identity();
                        p2p_adaptor.peers.insert(peer_id, new_router.clone());
                        // Register flows for the new peer
                        if let Ok(flow_terminates) =
                            flow_registry.initialize_flows(P2pConnection::new(new_router, p2p_adaptor.clone())).await
                        {
                            for (flow_id, flow_terminate) in flow_terminates {
                                let result = p2p_adaptor.flow_termination.insert(flow_id, flow_terminate);
                                if result.is_some() {
                                    panic!(
                                "At flow initialization, insertion into the map - got existing value, flow-key = router-id: {:?}",
                                result.unwrap().key()
                            );
                                }
                            }
                        }
                    }
                    P2pEvent::RouterClosing(peer_id) => {
                        p2p_adaptor.peers.remove(&peer_id);
                    }
                }
            }
        });
        Some(p2p_adaptor_clone)
    }

    /// Regular kaspa node will use this call to have both server & client connections
    async fn listen(ip_port: String, flow_registry: Arc<dyn FlowRegistryApi>) -> Option<Arc<Self>> {
        // [0] - Create new router - first instance
        // upper_layer_rx will be used to dispatch notifications about new-connections, both for client & server
        let (master_router, mut upper_layer_rx) = Router::new().await;
        // [1] - Start listener (de-facto Server side )
        let terminate_server = infra::P2pServer::listen(ip_port, master_router.clone(), true).await;
        // [2] - Check that server is ok & register termination signal ( as an example )
        if let Ok(t) = terminate_server {
            debug!("P2P, Server is running ...");
            let p2p_adaptor = Arc::new(P2pAdaptor {
                master_router,
                flow_termination: Map::new(),
                p2p_termination: Some(t),
                peers: Map::new(),
                outbound_clients: Map::new(),
            });
            let p2p_adaptor_clone = p2p_adaptor.clone();
            // [3] - Start service layer to listen when new connection is coming ( Server & Client side )
            tokio::spawn(async move {
                // loop will exit when all sender channels will be dropped
                // --> when all routers will be dropped & grpc-service will be stopped
                while let Some(new_event) = upper_layer_rx.recv().await {
                    match new_event {
                        P2pEvent::NewRouter(new_router) => {
                            // Insert the new router to the adaptor peer list
                            let peer_id = new_router.identity();
                            p2p_adaptor.peers.insert(peer_id, new_router.clone());
                            // Register flows for the new peer
                            if let Ok(flow_terminates) =
                                flow_registry.initialize_flows(P2pConnection::new(new_router, p2p_adaptor.clone())).await
                            {
                                for (flow_id, flow_terminate) in flow_terminates {
                                    let result = p2p_adaptor.flow_termination.insert(flow_id, flow_terminate);
                                    if result.is_some() {
                                        panic!(
                                    "At flow initialization, insertion into the map - got existing value, flow-key = router-id: {:?}",
                                    result.unwrap().key()
                                );
                                    }
                                }
                            }
                        }
                        P2pEvent::RouterClosing(peer_id) => {
                            p2p_adaptor.peers.remove(&peer_id);
                        }
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
        let client = P2pClient::connect_with_retry(ip_port, self.master_router.clone(), true, 16).await;
        match client {
            Some(connected_client) => {
                let peer_id = connected_client.identity();
                self.outbound_clients.insert(peer_id, connected_client);
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
            Some(peer) => {
                let result = peer.val().route_to_network(msg).await;
                if !result {
                    warn!("P2P, P2PAdaptor::send<T> - can't route message to peer-id: {:?}", id);
                }
            }
            None => {
                warn!("P2P, P2PAdaptor::send<T> - try to send message to peer that does not exist, peer-id: {:?}", id);
            }
        }
    }

    async fn terminate(&self, id: uuid::Uuid) {
        let is_peer = match self.peers.remove(&id) {
            Some(peer) => {
                peer.val().close().await;
                debug!("P2P, P2pAdaptor::terminate - peer-id: {:?}, is terminated", id);
                true
            }
            None => false,
        };
        if is_peer {
            // Some peers are outbound connections
            self.outbound_clients.remove(&id);
        } else {
            match self.flow_termination.remove(&id) {
                // Terminates by dropping the terminate sender
                Some(_flow_terminate_channel) => {
                    debug!("P2P, P2pAdaptor::terminate - flow-id: {:?}, is terminated", id);
                }
                None => {
                    warn!("P2P, P2pAdaptor::terminate - try to remove unknown peer/flow id: {:?}", id);
                }
            }
        }
    }

    async fn terminate_all_peers_and_flows(&self) {
        let peer_ids = self.get_all_peer_ids();
        for peer_id in peer_ids.iter() {
            match self.peers.remove(peer_id) {
                Some(peer) => {
                    peer.val().close().await;
                    debug!("P2P, P2pAdaptor::terminate_all_peers_and_flows - peer-id: {:?}, is terminated", peer_id);
                }
                None => {
                    warn!(
                        "P2P, P2pAdaptor::terminate_all_peers_and_flows - peer-id: {:?} removed in parallel by another thread",
                        peer_id
                    );
                }
            }
            // Some peers are outbound connections
            self.outbound_clients.remove(peer_id);
        }
        let flow_ids = self.get_all_flow_ids();
        for flow_id in flow_ids.iter() {
            match self.flow_termination.remove(flow_id) {
                // Terminates by dropping the terminate sender
                Some(_flow_terminate_channel) => {
                    debug!("P2P, P2pAdaptor::terminate_all_peers_and_flows - flow-id: {:?}, is terminated", flow_id);
                }
                None => {
                    warn!(
                        "P2P, P2pAdaptor::terminate_all_peers_and_flows - flow-id: {:?} removed in parallel by another thread",
                        flow_id
                    );
                }
            }
        }
    }

    fn get_all_peer_ids(&self) -> Vec<uuid::Uuid> {
        let mut ids = std::vec::Vec::<uuid::Uuid>::new();
        for peer in self.peers.iter() {
            ids.push(*peer.key());
        }
        ids
    }

    fn get_outbound_peer_ids(&self) -> Vec<uuid::Uuid> {
        let mut ids = std::vec::Vec::<uuid::Uuid>::new();
        for peer in self.outbound_clients.iter() {
            ids.push(*peer.key());
        }
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
