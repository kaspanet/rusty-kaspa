use std::net::SocketAddr;
use std::sync::{Arc, atomic::AtomicBool};

use kaspa_core::warn;
use kaspa_utils::triggers::SingleTrigger;
use tokio::{net::TcpListener, sync::mpsc};

use crate::servers::auth::TokenAuthenticator;
use crate::servers::peer_directory::PeerDirectory;

use crate::servers::tcp_control::{Hub, HubEvent, PeerDirection, TcpServer, tcp_connect};

// TODO: holds tcp server and hub
pub struct ControlRuntime {
    tcp_server: TcpServer,
    hub: Hub,
    shutdown: SingleTrigger,
    hub_sender: mpsc::UnboundedSender<HubEvent>,
    directory: Arc<PeerDirectory>,
    is_ready: Arc<AtomicBool>,
}

impl ControlRuntime {
    pub async fn new(
        listen_address: SocketAddr,
        directory: Arc<PeerDirectory>,
        authenticator: Arc<TokenAuthenticator>,
        is_ready: Arc<AtomicBool>, // this is the same bool that defines if udp runtime is on / off.
    ) -> Self {
        let (hub_sender, hub_receiver) = mpsc::unbounded_channel::<HubEvent>();
        let tcp_listener = TcpListener::bind(listen_address).await.unwrap();
        let shutdown = SingleTrigger::new();
        let tcp_server = TcpServer::new(tcp_listener, authenticator.clone(), hub_sender.clone(), shutdown.listener.clone(), directory.clone());
        let hub = Hub::new(directory.clone(), is_ready.clone(), hub_sender.clone(), hub_receiver);
        Self { tcp_server, hub, shutdown, hub_sender, directory, is_ready: is_ready.clone() }
    }

    pub async fn run(&mut self) {
        self.tcp_server.run().await;
        self.hub.run().await;
    }

    pub async fn stop(&mut self) {
        // below releases the tcp listen loop.
        self.shutdown.trigger.trigger();
        self.hub.shutdown_all_peers().await;
    }

    pub async fn connect_peer(
        &self,
        remote_addr: SocketAddr,
        direction: PeerDirection,
        local_udp_port: u16,
        authenticator: Arc<TokenAuthenticator>,
    ) {
        if let Err(e) = tcp_connect(remote_addr, authenticator, direction, local_udp_port, self.hub_sender.clone(), self.directory.allowlist()).await {
            warn!("Failed to connect to peer {}: {}", remote_addr, e);
        }
    }

    pub async fn signal_ready(&self) {
        self.hub.signal_ready().await;
    }

    pub async fn signal_not_ready(&self) {
        self.hub.signal_not_ready().await;
    }
}
