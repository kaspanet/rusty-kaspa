//! Transport selection and client construction.
//!
//! Produces an `Arc<dyn RpcApi>` from connection options, transparently
//! supporting both gRPC (`grpc://`) and wRPC (`ws://` / `wss://`).

use crate::error::{CliError, Result};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::notify::mode::NotificationMode;
use kaspa_wrpc_client::client::{ConnectOptions as WrpcConnectOptions, ConnectStrategy};
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use std::sync::Arc;
use std::time::Duration;

/// Wire transport to use.
#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Grpc,
    Wrpc,
}

/// Fully resolved connection parameters.
#[derive(Clone, Debug)]
pub struct ConnectOptions {
    /// Endpoint URL. When `None`, localhost with the default port is used.
    pub url: Option<String>,
    /// Network (selects default ports).
    pub network: NetworkId,
    /// Explicit transport override; when `None` it is inferred from the URL scheme.
    pub transport: Option<Transport>,
    /// wRPC encoding (ignored for gRPC).
    pub encoding: WrpcEncoding,
    /// Request/connect timeout in milliseconds.
    pub timeout_ms: u64,
}

impl ConnectOptions {
    /// Resolve the effective transport: explicit override, else inferred from
    /// the URL scheme, else wRPC by default.
    pub fn resolved_transport(&self) -> Transport {
        if let Some(t) = self.transport {
            return t;
        }
        if let Some(url) = &self.url {
            let lower = url.to_ascii_lowercase();
            if lower.starts_with("grpc://") {
                return Transport::Grpc;
            }
            if lower.starts_with("ws://") || lower.starts_with("wss://") {
                return Transport::Wrpc;
            }
        }
        Transport::Wrpc
    }
}

/// Default RPC port for the given network, transport and encoding.
fn default_port(net: NetworkType, transport: Transport, encoding: WrpcEncoding) -> u16 {
    match transport {
        Transport::Grpc => net.default_rpc_port(),
        Transport::Wrpc => match encoding {
            WrpcEncoding::Borsh => net.default_borsh_rpc_port(),
            WrpcEncoding::SerdeJson => net.default_json_rpc_port(),
        },
    }
}

/// Build a complete `scheme://host:port` URL, filling in the scheme (from the
/// transport) and the default port (from network, transport and encoding) when
/// omitted. The host has no sensible default, so a missing URL is a usage error
/// rather than a silent guess at `127.0.0.1`.
fn build_url(opts: &ConnectOptions, transport: Transport) -> Result<String> {
    let raw = opts.url.as_deref().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| {
        CliError::Usage(
            "no node URL specified: pass --url <URL> (for example `grpc://HOST:16110`, `ws://HOST:17110`, \
             or `wss://HOST`) or set `url` in the config file"
                .to_string(),
        )
    })?;

    let default_scheme = match transport {
        Transport::Grpc => "grpc",
        Transport::Wrpc => "ws",
    };
    let port = default_port(*opts.network, transport, opts.encoding);

    let (scheme, rest) = match raw.split_once("://") {
        Some((s, r)) => (s.to_string(), r),
        None => (default_scheme.to_string(), raw),
    };
    // A scheme with no authority (e.g. `grpc://`) gives no host to connect to.
    if rest.split(['/', ':']).next().unwrap_or("").is_empty() {
        return Err(CliError::Usage(format!("invalid --url {raw:?}: missing host")));
    }
    // Append the default port only when the authority lacks one.
    Ok(if rest.contains(':') { format!("{scheme}://{rest}") } else { format!("{scheme}://{rest}:{port}") })
}

/// Connect to a node and return a transport-agnostic RPC handle.
pub async fn connect(opts: &ConnectOptions) -> Result<Arc<dyn RpcApi>> {
    let transport = opts.resolved_transport();
    let url = build_url(opts, transport)?;

    match transport {
        Transport::Grpc => {
            let client = GrpcClient::connect_with_args(
                NotificationMode::Direct,
                url,
                None,  // subscription_context
                false, // reconnect
                None,  // connection_event_sender
                false, // override_handle_stop_notify
                Some(opts.timeout_ms),
                Default::default(), // counters
            )
            .await
            .map_err(|e| CliError::Connection(e.to_string()))?;
            Ok(Arc::new(client))
        }
        Transport::Wrpc => {
            let client = KaspaRpcClient::new(
                opts.encoding,
                Some(url.as_str()),
                None, // resolver (explicit url is used)
                None, // network_id (only needed with a resolver)
                None, // subscription_context
            )
            .map_err(|e| CliError::Connection(e.to_string()))?;

            let connect_options = WrpcConnectOptions {
                block_async_connect: true,
                connect_timeout: Some(Duration::from_millis(opts.timeout_ms)),
                strategy: ConnectStrategy::Fallback,
                ..Default::default()
            };
            client.connect(Some(connect_options)).await.map_err(|e| CliError::Connection(e.to_string()))?;
            Ok(Arc::new(client))
        }
    }
}
