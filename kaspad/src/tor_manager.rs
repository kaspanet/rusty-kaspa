use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    path::PathBuf,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use hex::encode as hex_encode;
use kaspa_core::{info, warn};
use thiserror::Error;
use tor_interface::{
    legacy_tor_client::{LegacyTorClient, LegacyTorClientConfig, TorAuth},
    tor_crypto::{Ed25519PrivateKey, V3OnionServiceId, X25519PublicKey},
    tor_provider::{OnionListener, TorEvent, TorProvider},
};

/// Arguments required to connect to (or launch) a Tor daemon using the legacy c-tor backend.
#[derive(Clone, Debug)]
pub struct TorSystemConfig {
    pub control_addr: SocketAddr,
    pub socks_addr: SocketAddr,
    pub auth: TorAuth,
    pub bootstrap_timeout: Duration,
}

/// Errors emitted by [`TorManager`].
#[derive(Debug, Error)]
pub enum TorManagerError {
    #[error("failed communicating with legacy tor daemon: {0}")]
    Legacy(#[from] tor_interface::legacy_tor_client::Error),
    #[error("tor provider error: {0}")]
    Provider(#[from] tor_interface::tor_provider::Error),
    #[error("tor crypto error: {0}")]
    Crypto(#[from] tor_interface::tor_crypto::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("tor control protocol error: {0}")]
    Control(String),
    #[error("tor bootstrap timed out after {0:?}")]
    BootstrapTimeout(Duration),
}

/// Thin wrapper around `tor-interface`'s [`LegacyTorClient`] with some kaspad-specific conveniences.
///
/// For now the manager only supports connecting to an already running tor daemon (matching Bitcoin Core's
/// system-tor integration). Future work will extend this to manage a bundled tor binary when one is not
/// present on the host.
pub struct TorManager {
    client: Mutex<LegacyTorClient>,
    socks_addr: SocketAddr,
    control_addr: SocketAddr,
    auth: TorAuth,
}

impl TorManager {
    /// Connect to an existing tor daemon, authenticate, and wait for bootstrap completion.
    pub fn connect_system(config: TorSystemConfig) -> Result<Self, TorManagerError> {
        let TorSystemConfig { control_addr, socks_addr, auth, bootstrap_timeout } = config;

        let mut client = LegacyTorClient::new(LegacyTorClientConfig::SystemTor {
            tor_socks_addr: socks_addr,
            tor_control_addr: control_addr,
            tor_control_auth: auth.clone(),
        })?;

        let version = client.version();
        info!("Connected to Tor daemon version {}", version);

        client.bootstrap()?;
        wait_for_bootstrap(&mut client, bootstrap_timeout)?;

        Ok(Self { client: Mutex::new(client), socks_addr, control_addr, auth })
    }

    /// Return the SOCKS listener address that should be supplied to outbound networking components.
    pub fn socks_addr(&self) -> SocketAddr {
        self.socks_addr
    }

    pub fn control_addr(&self) -> SocketAddr {
        self.control_addr
    }

    pub fn remove_hidden_service(&self, service_id: &V3OnionServiceId) -> Result<(), TorManagerError> {
        let mut stream = TcpStream::connect(self.control_addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        let mut reader = BufReader::new(stream.try_clone()?);

        self.authenticate_control(&mut stream, &mut reader)?;

        let command = format!("DEL_ONION {}\r\n", service_id);
        stream.write_all(command.as_bytes())?;

        loop {
            let mut line = String::new();
            let bytes = reader.read_line(&mut line)?;
            if bytes == 0 {
                return Err(TorManagerError::Control("unexpected EOF while waiting for DEL_ONION response".into()));
            }
            let trimmed = line.trim();
            if trimmed.starts_with("250 ") {
                break;
            } else if trimmed.starts_with('5') {
                return Err(TorManagerError::Control(trimmed.to_string()));
            }
        }

        Ok(())
    }

    /// Poll underlying tor events. Consumers should call this periodically to drain bootstrap/log events.
    pub fn update(&self) -> Result<Vec<TorEvent>, TorManagerError> {
        Ok(self.client.lock().unwrap().update()?)
    }

    /// Create a persistent onion service bound to the provided virtual port and local target.
    ///
    /// The manager expects the caller to take responsibility for running an application server on the
    /// returned [`OnionListener`]. The listener is configured in non-blocking mode by the caller.
    pub fn create_onion_listener(
        &mut self,
        private_key: &Ed25519PrivateKey,
        virt_port: u16,
        authorized_clients: Option<&[X25519PublicKey]>,
    ) -> Result<OnionListener, TorManagerError> {
        let listener = self.client.lock().unwrap().listener(private_key, virt_port, authorized_clients)?;
        Ok(listener)
    }

    /// Convenience for deriving the v3 onion identifier from a private key.
    pub fn onion_id_for(private_key: &Ed25519PrivateKey) -> V3OnionServiceId {
        V3OnionServiceId::from_private_key(private_key)
    }

    /// Load an Ed25519 onion service key from disk (c-tor key-blob format).
    pub fn load_onion_key(path: &PathBuf) -> Result<Ed25519PrivateKey, TorManagerError> {
        let blob = fs::read_to_string(path)?;
        Ok(Ed25519PrivateKey::from_key_blob(blob.trim())?)
    }

    pub fn save_onion_key(key: &Ed25519PrivateKey, path: &PathBuf) -> Result<(), TorManagerError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, key.to_key_blob())?;
        Ok(())
    }

    pub fn publish_hidden_service(
        &self,
        private_key: &Ed25519PrivateKey,
        virt_port: u16,
        target: SocketAddr,
    ) -> Result<V3OnionServiceId, TorManagerError> {
        let mut stream = TcpStream::connect(self.control_addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        let mut reader = BufReader::new(stream.try_clone()?);

        self.authenticate_control(&mut stream, &mut reader)?;

        let key_blob = private_key.to_key_blob();
        let target_repr = format_socket_addr(target);
        let command = format!("ADD_ONION {} Flags=Detach Port={},{}\r\n", key_blob, virt_port, target_repr);
        stream.write_all(command.as_bytes())?;

        let mut service_id: Option<String> = None;
        loop {
            let mut line = String::new();
            let bytes = reader.read_line(&mut line)?;
            if bytes == 0 {
                return Err(TorManagerError::Control("unexpected EOF while waiting for ADD_ONION response".into()));
            }
            let trimmed = line.trim();
            if trimmed.starts_with("250-ServiceID=") {
                service_id = Some(trimmed["250-ServiceID=".len()..].to_string());
            } else if trimmed.starts_with("250 ") {
                break;
            } else if trimmed.starts_with('5') {
                return Err(TorManagerError::Control(trimmed.to_string()));
            }
        }

        let service_id = service_id.ok_or_else(|| TorManagerError::Control("missing ServiceID in ADD_ONION reply".into()))?;
        Ok(V3OnionServiceId::from_string(&service_id)?)
    }

    fn authenticate_control(&self, stream: &mut TcpStream, reader: &mut BufReader<TcpStream>) -> Result<(), TorManagerError> {
        let command = match &self.auth {
            TorAuth::Null => "AUTHENTICATE\r\n".to_string(),
            TorAuth::Password(password) => format!("AUTHENTICATE \"{}\"\r\n", escape_control_password(password)),
            TorAuth::CookieFile(path) => {
                let cookie = fs::read(path)?;
                format!("AUTHENTICATE {}\r\n", hex_encode(cookie))
            }
        };

        stream.write_all(command.as_bytes())?;
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if !line.trim().starts_with("250") {
            return Err(TorManagerError::Control(format!("authentication failed: {}", line.trim())));
        }
        Ok(())
    }
}

fn wait_for_bootstrap(client: &mut LegacyTorClient, timeout: Duration) -> Result<(), TorManagerError> {
    let deadline = Instant::now() + timeout;
    let mut last_progress: Option<u32> = None;

    loop {
        for event in client.update()? {
            match event {
                TorEvent::BootstrapStatus { progress, tag, summary } => {
                    if last_progress != Some(progress) {
                        info!("Tor bootstrap {progress}% - {tag}: {summary}");
                        last_progress = Some(progress);
                    }
                }
                TorEvent::BootstrapComplete => {
                    info!("Tor bootstrap complete");
                    return Ok(());
                }
                TorEvent::LogReceived { line } => {
                    // Tor can be quite chatty; downgrade to debug once we have more granular logging controls.
                    warn!("tor: {}", line);
                }
                _ => {}
            }
        }

        if Instant::now() > deadline {
            return Err(TorManagerError::BootstrapTimeout(timeout));
        }

        thread::sleep(Duration::from_millis(200));
    }
}

fn escape_control_password(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

fn format_socket_addr(addr: SocketAddr) -> String {
    match addr.ip() {
        IpAddr::V6(v6) => format!("[{}]:{}", v6, addr.port()),
        IpAddr::V4(_) => format!("{}:{}", addr.ip(), addr.port()),
    }
}
