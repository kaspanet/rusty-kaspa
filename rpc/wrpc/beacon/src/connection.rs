use crate::imports::*;

const BIAS_SCALE: u64 = 1_000_000;

#[derive(Debug, Clone)]
pub struct Descriptor {
    pub connection: Arc<Connection>,
    pub json: String,
}

impl From<&Arc<Connection>> for Descriptor {
    fn from(connection: &Arc<Connection>) -> Self {
        Self { connection: connection.clone(), json: serde_json::to_string(&Output::from(connection)).unwrap() }
    }
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: [{:>3}] {}", self.node.id_string, self.clients(), self.node.address)
    }
}

#[derive(Debug)]
pub struct Connection {
    pub node: Arc<Node>,
    bias: u64,
    descriptor: RwLock<Option<Descriptor>>,
    sender: Sender<Params>,
    client: KaspaRpcClient,
    shutdown_ctl: DuplexChannel<()>,
    is_connected: Arc<AtomicBool>,
    is_synced: Arc<AtomicBool>,
    is_online: Arc<AtomicBool>,
    clients: Arc<AtomicU64>,
    args: Arc<Args>,
}

impl Connection {
    pub fn try_new(node: Arc<Node>, sender: Sender<Params>, args: &Arc<Args>) -> Result<Self> {
        let client = KaspaRpcClient::new(node.encoding, Some(&node.address))?;
        let descriptor = RwLock::default();
        let shutdown_ctl = DuplexChannel::oneshot();
        let is_connected = Arc::new(AtomicBool::new(false));
        let is_synced = Arc::new(AtomicBool::new(true));
        let is_online = Arc::new(AtomicBool::new(false));
        let clients = Arc::new(AtomicU64::new(0));
        let bias = (node.bias.unwrap_or(1.0) * BIAS_SCALE as f64) as u64;
        let args = args.clone();
        Ok(Self { node, descriptor, sender, client, shutdown_ctl, is_connected, is_synced, is_online, clients, bias, args })
    }

    pub fn verbose(&self) -> bool {
        self.args.verbose
    }

    pub fn score(&self) -> u64 {
        self.clients.load(Ordering::Relaxed) * self.bias / BIAS_SCALE
    }

    pub fn connected(&self) -> bool {
        self.is_connected.load(Ordering::Relaxed)
    }

    pub fn online(&self) -> bool {
        self.is_online.load(Ordering::Relaxed)
    }

    pub fn is_synced(&self) -> bool {
        self.is_synced.load(Ordering::Relaxed)
    }

    pub fn clients(&self) -> u64 {
        self.clients.load(Ordering::Relaxed)
    }

    pub fn status(&self) -> &'static str {
        if self.connected() {
            if self.is_synced() {
                "online"
            } else {
                "syncing"
            }
        } else {
            "offline"
        }
    }

    pub fn descriptor(&self) -> Option<Descriptor> {
        self.descriptor.read().unwrap().clone()
    }

    async fn connect(&self) -> Result<()> {
        let options = ConnectOptions { block_async_connect: false, strategy: ConnectStrategy::Retry, ..Default::default() };

        self.client.connect(Some(options)).await?;
        Ok(())
    }

    async fn task(self: Arc<Self>) -> Result<()> {
        self.connect().await?;
        let rpc_ctl_channel = self.client.rpc_ctl().multiplexer().channel();
        let shutdown_ctl_receiver = self.shutdown_ctl.request.receiver.clone();
        let shutdown_ctl_sender = self.shutdown_ctl.response.sender.clone();

        let interval = workflow_core::task::interval(Duration::from_secs(5));
        pin_mut!(interval);

        loop {
            select! {
                _ = interval.next().fuse() => {
                    if self.is_connected.load(Ordering::Relaxed) {
                        let previous = self.is_online.load(Ordering::Relaxed);
                        let online = self.update_metrics().await.is_ok();
                        self.is_online.store(online, Ordering::Relaxed);
                        if online != previous {
                            if self.verbose() {
                                log_error!("Offline","{}", self.node.address);
                            }
                            self.update(online).await?;
                        }
                    }
                }

                msg = rpc_ctl_channel.receiver.recv().fuse() => {
                    match msg {
                        Ok(msg) => {

                            // handle wRPC channel connection and disconnection events
                            match msg {
                                RpcState::Opened => {
                                    log_success!("Connected","{}",self.node.address);
                                    self.is_connected.store(true, Ordering::Relaxed);
                                    if self.update_metrics().await.is_ok() {
                                        self.is_online.store(true, Ordering::Relaxed);
                                        self.update(true).await?;
                                    } else {
                                        self.is_online.store(false, Ordering::Relaxed);
                                    }
                                },
                                RpcState::Closed => {
                                    self.is_connected.store(false, Ordering::Relaxed);
                                    self.is_online.store(false, Ordering::Relaxed);
                                    self.update(false).await?;
                                    log_error!("Disconnected","{}",self.node.address);
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

    pub fn start(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        spawn(async move {
            if let Err(error) = this.task().await {
                println!("NodeConnection task error: {:?}", error);
            }
        });

        Ok(())
    }

    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        self.shutdown_ctl.signal(()).await.expect("NodeConnection shutdown signal error");
        Ok(())
    }

    async fn update_metrics(self: &Arc<Self>) -> Result<bool> {
        match self.client.get_sync_status().await {
            Ok(is_synced) => {
                let previous_sync = self.is_synced.load(Ordering::Relaxed);
                self.is_synced.store(is_synced, Ordering::Relaxed);

                if is_synced {
                    match self.client.get_metrics(false, true, false, false).await {
                        Ok(metrics) => {
                            if let Some(connection_metrics) = metrics.connection_metrics {
                                // update
                                let previous = self.clients.load(Ordering::Relaxed);
                                let clients =
                                    connection_metrics.borsh_live_connections as u64 + connection_metrics.json_live_connections as u64;
                                self.clients.store(clients, Ordering::Relaxed);
                                if clients != previous {
                                    if self.verbose() {
                                        log_success!("Clients", "{self}");
                                    }
                                    Ok(true)
                                } else {
                                    Ok(false)
                                }
                            } else {
                                log_error!("Metrics", "{self} - failure");
                                Err(Error::ConnectionMetrics)
                            }
                        }
                        Err(err) => {
                            log_error!("Metrics", "{self}");
                            log_error!("RPC", "{err}");
                            Err(Error::Metrics)
                        }
                    }
                } else {
                    if is_synced != previous_sync {
                        log_error!("Syncing", "{self}");
                    }
                    Err(Error::Sync)
                }
            }
            Err(err) => {
                log_error!("RPC", "{self}");
                log_error!("RPC", "{err}");
                Err(Error::Status)
            }
        }
    }

    pub async fn update(self: &Arc<Self>, online: bool) -> Result<()> {
        *self.descriptor.write().unwrap() = online.then_some(self.into());
        self.sender.try_send(self.node.params())?;
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Output<'a> {
    pub id: &'a str,
    pub url: &'a str,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_link: Option<&'a str>,
}

impl<'a> From<&'a Arc<Connection>> for Output<'a> {
    fn from(connection: &'a Arc<Connection>) -> Self {
        let id = connection.node.id_string.as_str();
        let url = connection.node.address.as_str();
        let provider_name = connection.node.provider.as_deref();
        let provider_link = connection.node.link.as_deref();
        Self { id, url, provider_name, provider_link }
    }
}
