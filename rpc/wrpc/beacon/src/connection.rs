use crate::imports::*;

const BIAS_SCALE: u64 = 1_000_000;

#[derive(Debug)]
pub struct Connection {
    pub node: Arc<Node>,
    bias: u64,
    output: RwLock<String>,
    sender: Sender<Params>,
    client: KaspaRpcClient,
    shutdown_ctl: DuplexChannel<()>,
    connected: Arc<AtomicBool>,
    is_synced: Arc<AtomicBool>,
    online: Arc<AtomicBool>,
    clients: Arc<AtomicU64>,
    verbose: bool,
}

impl Connection {
    pub fn try_new(node: Arc<Node>, sender: Sender<Params>, verbose: bool) -> Result<Self> {
        let client = KaspaRpcClient::new(node.encoding, Some(&node.address))?;
        let output = RwLock::new(String::default());
        let shutdown_ctl = DuplexChannel::oneshot();
        let connected = Arc::new(AtomicBool::new(false));
        let syncing = Arc::new(AtomicBool::new(false));
        let online = Arc::new(AtomicBool::new(false));
        let clients = Arc::new(AtomicU64::new(0));
        let bias = (node.bias.unwrap_or(1.0) * BIAS_SCALE as f64) as u64;
        Ok(Self { node, output, sender, client, shutdown_ctl, connected, is_synced: syncing, online, clients, bias, verbose })
    }

    pub fn score(&self) -> u64 {
        self.clients.load(Ordering::Relaxed) * self.bias / BIAS_SCALE
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn online(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }

    pub fn is_synced(&self) -> bool {
        self.is_synced.load(Ordering::Relaxed)
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

    pub fn output(&self) -> String {
        self.output.read().unwrap().clone()
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
                    if self.connected.load(Ordering::Relaxed) {
                        let online = self.update_metrics().await.is_ok();
                        self.online.store(online, Ordering::Relaxed);
                        // if !online {
                        //     log_warn!("Offline","{}", self.node.address);
                        // }
                    }
                }

                msg = rpc_ctl_channel.receiver.recv().fuse() => {
                    match msg {
                        Ok(msg) => {

                            // handle RPC channel connection and disconnection events

                            match msg {
                                RpcState::Opened => {
                                    log_success!("Connected","{}",self.node.address);
                                    self.connected.store(true, Ordering::Relaxed);
                                    let online = self.update_metrics().await.is_ok();
                                    self.online.store(online, Ordering::Relaxed);
                                },
                                RpcState::Closed => {
                                    self.connected.store(false, Ordering::Relaxed);
                                    self.online.store(false, Ordering::Relaxed);
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

    async fn update_metrics(self: &Arc<Self>) -> Result<()> {
        if let Ok(is_synced) = self.client.get_sync_status().await {
            self.is_synced.store(is_synced, Ordering::Relaxed);

            if is_synced {
                if let Ok(metrics) = self.client.get_metrics(false, true, false, false).await {
                    if let Some(connection_metrics) = metrics.connection_metrics {
                        // update
                        let previous = self.clients.load(Ordering::Relaxed);
                        let clients =
                            connection_metrics.borsh_live_connections as u64 + connection_metrics.json_live_connections as u64;
                        self.clients.store(clients, Ordering::Relaxed);
                        if clients != previous {
                            if self.verbose {
                                log_success!("Clients", "[{clients:>3}] {}", self.node.address);
                            }
                            self.update_output().await?;
                        }
                        // log_success!("Online","[{clients:>3}] {}", self.node.address);
                        Ok(())
                    } else {
                        log_error!("Metrics", "{} - failure", self.node.address);
                        Err(Error::ConnectionMetrics)
                    }
                } else {
                    log_error!("Metrics", "{} - failure", self.node.address);
                    Err(Error::Metrics)
                }
            } else {
                // log_warn!("Syncing","{}", self.node.address);
                Err(Error::Sync)
            }
        } else {
            log_error!("Sync", "{} - failure", self.node.address);
            Err(Error::Status)
        }
    }

    pub async fn update_output(self: &Arc<Self>) -> Result<()> {
        let output = serde_json::to_string(&Output::from(self))?;
        *self.output.write().unwrap() = output;
        self.sender.try_send(self.node.params())?;
        Ok(())
    }
}

#[derive(Serialize)]
pub struct Output<'a> {
    pub url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<&'a str>,
    pub id: &'a str,
}

impl<'a> From<&'a Arc<Connection>> for Output<'a> {
    fn from(connection: &'a Arc<Connection>) -> Self {
        let url = connection.node.address.as_str();
        let provider = connection.node.provider.as_deref();
        let id = connection.node.id.as_str();
        Self { url, provider, id }
    }
}
