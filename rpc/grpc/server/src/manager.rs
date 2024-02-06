use crate::connection::{Connection, ConnectionId};
use kaspa_core::{debug, info, warn};
use kaspa_notify::connection::Connection as ConnectionT;
use parking_lot::RwLock;
use std::{
    collections::{hash_map::Entry::Occupied, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use thiserror::Error;
use tokio::sync::oneshot::Sender as OneshotSender;
use tokio::{sync::mpsc::Receiver as MpscReceiver, time::sleep};

#[derive(Debug, Error)]
pub(crate) enum RegistrationError {
    #[error("reached connection capacity of {0}")]
    CapacityReached(usize),
}
pub(crate) type RegistrationResult = Result<(), RegistrationError>;

pub(crate) struct RegistrationRequest {
    connection: Connection,
    response_sender: OneshotSender<RegistrationResult>,
}

impl RegistrationRequest {
    pub fn new(connection: Connection, response_sender: OneshotSender<RegistrationResult>) -> Self {
        Self { connection, response_sender }
    }
}

pub(crate) enum ManagerEvent {
    NewConnection(RegistrationRequest),
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
                    ManagerEvent::NewConnection(RegistrationRequest { connection, response_sender }) => {
                        match response_sender.send(self.register(connection.clone())) {
                            Ok(()) => {}
                            Err(_) => {
                                warn!("GRPC, registration of incoming connection {} failed", connection);
                                self.unregister(connection);
                            }
                        }
                    }
                    ManagerEvent::ConnectionClosing(connection) => {
                        self.unregister(connection);
                    }
                }
            }
            debug!("GRPC, Manager event loop exiting");
        });
    }

    fn register(&self, connection: Connection) -> RegistrationResult {
        let mut connections_write = self.connections.write();

        // Check if there is room for a new connection
        if connections_write.len() >= self.max_connections {
            return Err(RegistrationError::CapacityReached(self.max_connections));
        }

        debug!("GRPC, Registering a new connection from {connection}");
        let previous_connection = connections_write.insert(connection.identity(), connection.clone());
        info!("GRPC, new incoming connection {} #{}", connection, connections_write.len());

        // Release the write lock to prevent a deadlock if a previous connection exists and must be closed
        drop(connections_write);

        // A previous connection with the same id is VERY unlikely to occur but just in case, we close it cleanly
        if let Some(previous_connection) = previous_connection {
            previous_connection.close();
            warn!("GRPC, removing connection with duplicate identity: {}", previous_connection.identity());
        }

        Ok(())
    }

    fn unregister(&self, connection: Connection) {
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
    pub async fn terminate_all_connections(&self) {
        let mut closed_connections = HashSet::with_capacity(self.connections.read().len());
        loop {
            if let Some((id, connection)) = self
                .connections
                .read()
                .iter()
                .filter(|(id, _)| !closed_connections.contains(*id))
                .map(|(id, cx)| (*id, cx.clone()))
                .next()
            {
                closed_connections.insert(id);
                connection.close();
                continue;
            } else if self.connections.read().is_empty() {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    }

    /// Returns a list of all currently active connections (for unit tests only)
    #[cfg(test)]
    pub(crate) fn active_connections(&self) -> Vec<std::net::SocketAddr> {
        self.connections.read().values().map(|r| r.net_address()).collect()
    }

    /// Returns whether there are currently active connections (for unit tests only)
    #[cfg(test)]
    pub(crate) fn has_connections(&self) -> bool {
        !self.connections.read().is_empty()
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        debug!("GRPC, Dropping Manager, refs count {}", Arc::strong_count(&self.connections));
    }
}
