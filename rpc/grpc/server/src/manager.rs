use crate::connection::Connection;
use kaspa_core::debug;
use kaspa_notify::connection::Connection as ConnectionT;
use parking_lot::Mutex;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

#[derive(Clone, Debug)]
pub struct Manager {
    connections: Arc<Mutex<HashMap<SocketAddr, Connection>>>,
    max_connections: usize,
}

impl Manager {
    pub fn new(max_connections: usize) -> Self {
        Self { connections: Arc::new(Mutex::new(HashMap::new())), max_connections }
    }

    pub fn register(&self, connection: Connection) {
        debug!("gRPC: Register a new connection from {connection}");
        self.connections.lock().insert(connection.identity(), connection).map(|x| x.close());
    }

    pub fn is_full(&self) -> bool {
        self.connections.lock().len() >= self.max_connections
    }

    pub fn unregister(&self, net_address: SocketAddr) {
        match self.connections.lock().remove(&net_address) {
            Some(connection) => {
                debug!("gRPC: Unregister the gRPC connection from {connection}");
            }
            None => {
                debug!("gRPC: Unregister the gRPC connection from {net_address} failed for address not found");
            }
        }
    }

    pub fn terminate_all_connections(&self) {
        let connections = self.connections.lock().drain().map(|(_, r)| r).collect::<Vec<_>>();
        for connection in connections {
            connection.close();
        }
    }
}
