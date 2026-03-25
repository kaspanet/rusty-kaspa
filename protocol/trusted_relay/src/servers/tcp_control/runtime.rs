use std::net::SocketAddr;
use std::sync::Arc;

use kaspa_core::{info, warn};
use kaspa_utils::triggers::SingleTrigger;
use tokio::sync::mpsc;

use crate::servers::auth::TokenAuthenticator;
use crate::servers::peer_directory::PeerDirectory;

use crate::servers::tcp_control::{Hub, HubEvent, PeerDirection, TcpServer, tcp_connect};

/// Handle for controlling the TCP runtime after it's spawned.
/// Cheap to clone - all clones control the same runtime.
#[derive(Clone)]
pub struct ControlRuntimeHandle {
    hub_sender: mpsc::UnboundedSender<HubEvent>,
    shutdown: SingleTrigger,
    directory: Arc<PeerDirectory>,
}

impl ControlRuntimeHandle {
    /// Signal that UDP is ready - broadcast Start to all peers.
    pub fn signal_ready(&self) {
        let _ = self.hub_sender.send(HubEvent::SetLocalReady(true));
    }

    /// Signal that UDP is not ready - broadcast Stop to all peers.
    pub fn signal_not_ready(&self) {
        let _ = self.hub_sender.send(HubEvent::SetLocalReady(false));
    }

    /// Trigger shutdown of the runtime.
    pub fn stop(&self) {
        let _ = self.hub_sender.send(HubEvent::Shutdown);
        self.shutdown.trigger.trigger();
    }

    /// Connect to a peer.
    pub async fn connect_peer(
        &self,
        remote_addr: SocketAddr,
        direction: PeerDirection,
        local_udp_port: u16,
        authenticator: Arc<TokenAuthenticator>,
    ) {
        if let Err(e) =
            tcp_connect(remote_addr, authenticator, direction, local_udp_port, self.hub_sender.clone(), self.directory.allowlist())
                .await
        {
            warn!("Failed to connect to peer {}: {}", remote_addr, e);
        }
    }
}

/// TCP control runtime. Call `spawn()` to start, returns a handle for control.
pub struct ControlRuntime {
    tcp_server: TcpServer,
    hub: Hub,
    shutdown: SingleTrigger,
    hub_sender: mpsc::UnboundedSender<HubEvent>,
    directory: Arc<PeerDirectory>,
}

impl ControlRuntime {
    pub fn new(listen_address: SocketAddr, directory: Arc<PeerDirectory>, authenticator: Arc<TokenAuthenticator>) -> Self {
        let (hub_sender, hub_receiver) = mpsc::unbounded_channel::<HubEvent>();
        info!("Creating TCP control runtime on {}", listen_address);
        let listen_address = format!("0.0.0.0:{}", listen_address.port()).parse().expect("failed to parse listen address");
        let shutdown = SingleTrigger::new();
        let tcp_server =
            TcpServer::new(listen_address, authenticator.clone(), hub_sender.clone(), shutdown.listener.clone(), directory.clone());
        let hub = Hub::new(directory.clone(), hub_sender.clone(), hub_receiver);
        Self { tcp_server, hub, shutdown, hub_sender, directory }
    }

    /// Create a handle for controlling the runtime.
    pub fn handle(&self) -> ControlRuntimeHandle {
        ControlRuntimeHandle {
            hub_sender: self.hub_sender.clone(),
            shutdown: self.shutdown.clone(),
            directory: self.directory.clone(),
        }
    }

    /// Spawn the runtime as a background task. Returns a handle for control.
    pub fn spawn(self) -> (ControlRuntimeHandle, tokio::task::JoinHandle<()>) {
        let handle = self.handle();
        let task = tokio::spawn(self.run());
        (handle, task)
    }

    /// Run both tcp_server and hub concurrently until shutdown.
    async fn run(mut self) {
        info!("TCP control runtime starting");
        tokio::join!(
            self.tcp_server.run(),
            self.hub.run()
        );
        info!("TCP control runtime stopped");
    }
}
