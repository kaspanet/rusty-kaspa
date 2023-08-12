use crate::connection::Connection;
use kaspa_core::{debug, info, warn};
use kaspa_notify::connection::Connection as ConnectionT;
use parking_lot::RwLock;
use std::{
    collections::{hash_map::Entry::Occupied, HashMap},
    net::SocketAddr,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct Manager {
    connections: Arc<RwLock<HashMap<SocketAddr, Connection>>>,
    max_connections: usize,
}

impl Manager {
    pub fn new(max_connections: usize) -> Self {
        Self { connections: Arc::new(RwLock::new(HashMap::new())), max_connections }
    }

    pub fn register(&self, connection: Connection) {
        debug!("gRPC: registering a new connection from {connection}");
        let mut connections_write = self.connections.write();
        let previous_connection = connections_write.insert(connection.identity(), connection.clone());
        info!("gRPC: new incoming connection {} #{}", connection, connections_write.len());

        // Release the write lock to prevent a deadlock if a previous connection exists and must be closed
        drop(connections_write);

        if let Some(previous_connection) = previous_connection {
            previous_connection.close();
            warn!("gRPC: removing connection with duplicate identity: {}", previous_connection.identity());
        }
    }

    pub fn is_full(&self) -> bool {
        self.connections.read().len() >= self.max_connections
    }

    pub fn unregister(&self, connection: Connection) {
        if let Occupied(entry) = self.connections.write().entry(connection.identity()) {
            // We search for the connection by identity, but make sure to delete it only if it's actually the same object.
            // This is extremely important in cases of duplicate connection rejection etc.
            if Connection::ptr_eq(entry.get(), &connection) {
                entry.remove_entry();
                debug!("gRPC: unregistering connection from {connection}");
            }
        }
    }

    /// Terminate all connections
    pub fn terminate_all_connections(&self) {
        let connections = self.connections.write().drain().map(|(_, r)| r).collect::<Vec<_>>();
        for connection in connections {
            connection.close();
        }
    }

    /// Returns a list of all currently active connections
    pub fn active_connections(&self) -> Vec<SocketAddr> {
        self.connections.read().values().map(|r| r.net_address()).collect()
    }

    /// Returns whether there are currently active connections
    pub fn has_connections(&self) -> bool {
        !self.connections.read().is_empty()
    }

    /// Returns whether a connection matching `net_address` is registered
    pub fn has_connection(&self, net_address: SocketAddr) -> bool {
        self.connections.read().contains_key(&net_address)
    }
}
