use crate::connection::{Connection, Descriptor};
use crate::imports::*;

static MONITOR: OnceLock<Arc<Monitor>> = OnceLock::new();

pub fn init(args: &Arc<Args>) {
    MONITOR.set(Arc::new(Monitor::new(args))).unwrap();
}

pub fn monitor() -> &'static Arc<Monitor> {
    MONITOR.get().unwrap()
}

pub async fn start() -> Result<()> {
    monitor().start().await
}

pub async fn stop() -> Result<()> {
    monitor().stop().await
}

/// Monitor receives updates from [Connection] monitoring tasks
/// and updates the descriptors for each [Params] based on the
/// connection store (number of connections * bias).
pub struct Monitor {
    args: Arc<Args>,
    connections: RwLock<AHashMap<PathParams, Vec<Arc<Connection>>>>,
    descriptors: RwLock<AHashMap<PathParams, Descriptor>>,
    channel: Channel<PathParams>,
    shutdown_ctl: DuplexChannel<()>,
}

impl Default for Monitor {
    fn default() -> Self {
        Self {
            args: Arc::new(Args::default()),
            connections: Default::default(),
            descriptors: Default::default(),
            channel: Channel::unbounded(),
            shutdown_ctl: DuplexChannel::oneshot(),
        }
    }
}

impl fmt::Debug for Monitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Monitor")
            .field("verbose", &self.verbose())
            .field("connections", &self.connections)
            .field("descriptors", &self.descriptors)
            .finish()
    }
}

impl Monitor {
    pub fn new(args: &Arc<Args>) -> Self {
        Self { args: args.clone(), ..Default::default() }
    }

    pub fn verbose(&self) -> bool {
        self.args.verbose
    }

    pub fn connections(&self) -> AHashMap<PathParams, Vec<Arc<Connection>>> {
        self.connections.read().unwrap().clone()
    }

    /// Process an update to `Server.toml` removing or adding node connections accordingly.
    pub async fn update_nodes(&self, nodes: Vec<Arc<Node>>) -> Result<()> {
        let mut connections = self.connections();

        for params in PathParams::iter() {
            let nodes = nodes.iter().filter(|node| node.params() == params).collect::<Vec<_>>();

            let list = connections.entry(params).or_default();

            let create: Vec<_> = nodes.iter().filter(|node| !list.iter().any(|connection| connection.node == ***node)).collect();

            let remove: Vec<_> =
                list.iter().filter(|connection| !nodes.iter().any(|node| connection.node == **node)).cloned().collect();

            for node in create {
                let created = Arc::new(Connection::try_new((*node).clone(), self.channel.sender.clone(), &self.args)?);
                created.start()?;
                list.push(created);
            }

            for removed in remove {
                removed.stop().await?;
                list.retain(|c| c.node != removed.node);
            }
        }

        *self.connections.write().unwrap() = connections;

        // flush all params to the update channel to refresh selected descriptors
        PathParams::iter().for_each(|param| self.channel.sender.try_send(param).unwrap());

        Ok(())
    }

    pub async fn start(self: &Arc<Self>) -> Result<()> {
        let toml = std::fs::read_to_string(Path::new("Servers.toml"))?;
        let nodes = crate::node::try_parse_nodes(toml.as_str())?;

        let this = self.clone();
        spawn(async move {
            if let Err(error) = this.task().await {
                println!("NodeConnection task error: {:?}", error);
            }
        });

        self.update_nodes(nodes).await?;

        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.shutdown_ctl.signal(()).await.expect("Monitor shutdown signal error");
        Ok(())
    }

    async fn task(self: Arc<Self>) -> Result<()> {
        let receiver = self.channel.receiver.clone();
        let shutdown_ctl_receiver = self.shutdown_ctl.request.receiver.clone();
        let shutdown_ctl_sender = self.shutdown_ctl.response.sender.clone();

        loop {
            select! {

                msg = receiver.recv().fuse() => {
                    match msg {
                        Ok(params) => {

                            // run node elections

                            let mut connections = self.connections()
                                .get(&params)
                                .expect("Monitor: expecting existing connection params")
                                .clone()
                                .into_iter()
                                .filter(|connection|connection.online())
                                .collect::<Vec<_>>();
                            if connections.is_empty() {
                                self.descriptors.write().unwrap().remove(&params);
                            } else {
                                connections.sort_by_key(|connection| connection.score());

                                if self.args.election {
                                    log_success!("","");
                                    connections.iter().for_each(|connection| {
                                        log_warn!("Node","{}", connection);
                                    });
                                }

                                if let Some(descriptor) = connections.first().unwrap().descriptor() {
                                    let mut descriptors = self.descriptors.write().unwrap();

                                    // extra debug output & monitoring
                                    if self.args.verbose || self.args.election {
                                        if let Some(current) = descriptors.get(&params) {
                                            if current.connection.node.id != descriptor.connection.node.id {
                                                log_success!("Election","{}", descriptor.connection);
                                                descriptors.insert(params,descriptor);
                                            } else {
                                                log_success!("Keep","{}", descriptor.connection);
                                            }
                                        } else {
                                            log_success!("Default","{}", descriptor.connection);
                                            descriptors.insert(params,descriptor);
                                        }
                                    } else {
                                        descriptors.insert(params,descriptor);
                                    }
                                }

                                if self.args.election && self.args.verbose {
                                    log_success!("","");
                                }
                            }
                        }
                        Err(err) => {
                            println!("Monitor: error while receiving update message: {err}");
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

    /// Get the status of all nodes as a JSON string (available via `/status` endpoint if enabled).
    pub fn get_all_json(&self) -> String {
        let connections = self.connections();
        let nodes = connections.values().flatten().map(Status::from).collect::<Vec<_>>();
        serde_json::to_string(&nodes).unwrap()
    }

    /// Get JSON string representing node information (id, url, provider, link)
    pub fn get_json(&self, params: &PathParams) -> Option<String> {
        self.descriptors.read().unwrap().get(params).cloned().map(|descriptor| descriptor.json)
    }
}

#[derive(Serialize)]
pub struct Status<'a> {
    pub id: &'a str,
    pub url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_url: Option<&'a str>,
    pub transport: Transport,
    pub encoding: WrpcEncoding,
    pub network: NetworkId,
    pub online: bool,
    pub status: &'static str,
}

impl<'a> From<&'a Arc<Connection>> for Status<'a> {
    fn from(connection: &'a Arc<Connection>) -> Self {
        let url = connection.node.address.as_str();
        let provider_name = connection.node.provider.as_ref().map(|provider| provider.name.as_str());
        let provider_url = connection.node.provider.as_ref().map(|provider| provider.url.as_str());
        let id = connection.node.id_string.as_str();
        let transport = connection.node.transport;
        let encoding = connection.node.encoding;
        let network = connection.node.network;
        let status = connection.status();
        let online = connection.online();
        Self { id, url, provider_name, provider_url, transport, encoding, network, status, online }
    }
}
