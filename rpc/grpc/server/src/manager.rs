use crate::connection::{Connection, ConnectionId};
use itertools::Itertools;
use kaspa_core::{debug, info, warn};
use kaspa_notify::connection::Connection as ConnectionT;
use parking_lot::RwLock;
use std::{
    collections::{hash_map::Entry::Occupied, HashMap},
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::mpsc::Receiver as MpscReceiver;
use tokio::sync::oneshot::Sender as OneshotSender;

#[derive(Debug)]
pub(crate) enum ManagerEvent {
    IsFull(OneshotSender<bool>),
    NewConnection(Connection),
    ConnectionClosing(Connection),
}

#[derive(Clone, Debug)]
pub struct Manager {
    connections: Arc<RwLock<HashMap<ConnectionId, Connection>>>,
    max_connections: usize,
}

impl Manager {
    pub fn new(max_connections: usize) -> Self {
        Self { connections: Default::default(), max_connections }
    }

    /// Starts a loop for receiving central manager events from all connections. This mechanism is used for
    /// managing a collection of active connections.
    pub(crate) fn start_event_loop(self, mut manager_receiver: MpscReceiver<ManagerEvent>) {
        debug!("GRPC, Manager event loop starting");
        tokio::spawn(async move {
            while let Some(new_event) = manager_receiver.recv().await {
                match new_event {
                    ManagerEvent::IsFull(sender) => {
                        // The receiver of this channel may have been dropped in the
                        // meantime so we ignore the result of the send.
                        let _ = sender.send(self.is_full());
                    }
                    ManagerEvent::NewConnection(new_connection) => {
                        self.register(new_connection);
                    }
                    ManagerEvent::ConnectionClosing(connection) => {
                        self.unregister(connection);
                    }
                }
            }
            debug!("GRPC, Manager event loop exiting");
        });
    }

    pub fn register(&self, connection: Connection) {
        debug!("GRPC, Registering a new connection from {connection}");
        let mut connections_write = self.connections.write();
        let previous_connection = connections_write.insert(connection.identity(), connection.clone());
        info!("GRPC, new incoming connection {} #{}", connection, connections_write.len());

        // Release the write lock to prevent a deadlock if a previous connection exists and must be closed
        drop(connections_write);

        // A previous connection with the same id is VERY unlikely to occur but just in case, we close it cleanly
        if let Some(previous_connection) = previous_connection {
            previous_connection.close();
            warn!("GRPC, removing connection with duplicate identity: {}", previous_connection.identity());
        }
    }

    pub fn is_full(&self) -> bool {
        self.connections.read().len() >= self.max_connections
    }

    pub fn unregister(&self, connection: Connection) {
        let mut connections_write = self.connections.write();
        let connection_count = connections_write.len();
        if let Occupied(entry) = connections_write.entry(connection.identity()) {
            // We search for the connection by identity, but make sure to delete it only if it's actually the same object.
            // This is extremely important in cases of duplicate connection rejection etc.
            if Connection::ptr_eq(entry.get(), &connection) {
                entry.remove_entry();
                info!("GRPC, end connection {} #{}", connection, connection_count);
            }
        }
    }

    /// Terminate all connections
    pub fn terminate_all_connections(&self) {
        // Note that using drain here prevents unregister() to successfully find the entry...
        let connections = self.connections.write().drain().map(|(_, cx)| cx).collect_vec();
        for (i, connection) in connections.into_iter().enumerate().rev() {
            connection.close();
            // ... so we log explicitly here
            info!("GRPC, end connection {} #{}", connection, i + 1);
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
}

impl Drop for Manager {
    fn drop(&mut self) {
        debug!("GRPC, Dropping Manager, refs count {}", Arc::strong_count(&self.connections));
    }
}
