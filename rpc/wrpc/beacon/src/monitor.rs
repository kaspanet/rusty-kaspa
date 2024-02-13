use crate::node::Node;
use crate::result::Result;
use futures::{select, FutureExt};
use kaspa_rpc_core::api::ctl::RpcState;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    client::{ConnectOptions, ConnectStrategy},
    KaspaRpcClient,
};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use workflow_core::channel::*;
use workflow_core::task::spawn;

pub fn monitor() -> &'static Monitor {
    static MONITOR: OnceLock<Monitor> = OnceLock::new();
    MONITOR.get_or_init(Monitor::default)
}

pub struct NodeConnection {
    node: Arc<Node>,
    client: KaspaRpcClient,
    shutdown_ctl: DuplexChannel<()>,
    connected: Arc<AtomicBool>,
    clients: Arc<AtomicU64>,
}

impl NodeConnection {
    pub fn try_new(node: Arc<Node>) -> Result<Self> {
        let client = KaspaRpcClient::new(node.protocol, Some(&node.address))?;
        let shutdown_ctl = DuplexChannel::oneshot();
        let connected = Arc::new(AtomicBool::new(false));
        let clients = Arc::new(AtomicU64::new(0));
        Ok(Self { node, client, shutdown_ctl, connected, clients })
    }

    async fn connect(&self) -> Result<()> {
        let options = ConnectOptions { block_async_connect: false, strategy: ConnectStrategy::Retry, ..Default::default() };

        self.client.connect(options).await?;
        Ok(())
    }

    async fn task(self: Arc<Self>) -> Result<()> {
        self.connect().await?;
        let rpc_ctl_channel = self.client.rpc_ctl().multiplexer().channel();
        let shutdown_ctl_receiver = self.shutdown_ctl.request.receiver.clone();
        let shutdown_ctl_sender = self.shutdown_ctl.response.sender.clone();

        loop {
            select! {

                msg = rpc_ctl_channel.receiver.recv().fuse() => {
                    match msg {
                        Ok(msg) => {

                            // handle RPC channel connection and disconnection events

                            match msg {
                                RpcState::Opened => {
                                    println!("Connected to {}",self.node.address);
                                    if self.update_metrics().await {
                                        self.connected.store(true, Ordering::Relaxed);
                                    }
                                },
                                RpcState::Closed => {
                                    self.connected.store(true, Ordering::Relaxed);
                                    println!("Disconnected from {}",self.node.address);
                                }
                            }
                        }
                        Err(err) => {
                            println!("Monitor: error while receiving rpc_ctl_channel message: {err}");
                            break;
                        }
                    }
                }

                _ = shutdown_ctl_receiver.recv().fuse() => {
                    break;
                },

            }
        }

        shutdown_ctl_sender.send(()).await.unwrap();

        Ok(())
    }

    fn start(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        spawn(async move {
            if let Err(error) = this.task().await {
                println!("NodeConnection task error: {:?}", error);
            }
        });

        Ok(())
    }

    async fn stop(self: &Arc<Self>) -> Result<()> {
        self.shutdown_ctl.signal(()).await.expect("NodeConnection shutdown signal error");
        Ok(())
    }

    async fn update_metrics(&self) -> bool {
        if let Ok(metrics) = self.client.get_metrics(false, true, false, false).await {
            if let Some(connection_metrics) = metrics.connection_metrics {
                // update

                let clients = connection_metrics.borsh_live_connections as u64 + connection_metrics.json_live_connections as u64;
                self.clients.store(clients, Ordering::Relaxed);
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}

#[derive(Default)]
pub struct Inner {
    connections: Vec<Arc<NodeConnection>>,
}

#[derive(Clone, Default)]
pub struct Monitor {
    inner: Arc<Mutex<Inner>>,
}

impl Monitor {
    // TODO: remove this
    #[allow(unused)]
    pub fn new() -> Self {
        Self { ..Default::default() }
    }

    pub fn connections(&self) -> Vec<Arc<NodeConnection>> {
        self.inner.lock().unwrap().connections.clone()
    }

    pub async fn update_nodes(&self, nodes: Vec<Arc<Node>>) -> Result<()> {
        let mut connections = self.connections();

        let create: Vec<_> = nodes
            .iter()
            .filter_map(|node| if !connections.iter().any(|connection| connection.node == *node) { Some(node.clone()) } else { None })
            .collect();

        let remove: Vec<_> = connections
            .iter()
            .filter_map(|connection| if !nodes.iter().any(|node| connection.node == *node) { Some(connection.clone()) } else { None })
            .collect();

        for node in create {
            let created = Arc::new(NodeConnection::try_new(Arc::clone(&node))?);
            created.start()?;
            connections.push(created);
        }

        for removed in remove {
            removed.stop().await?;
            connections.retain(|c| c.node != removed.node);
        }

        self.inner.lock().unwrap().connections = connections;

        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        // let toml = include_str!("../Servers.toml");
        // let nodes = crate::node::try_parse_nodes(toml)?;

        // self.update_nodes(nodes).await?;

        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        Ok(())
    }
}
