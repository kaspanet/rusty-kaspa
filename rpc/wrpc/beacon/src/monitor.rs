use crate::connection::Connection;
use crate::imports::*;

static MONITOR: OnceLock<Arc<Monitor>> = OnceLock::new();

pub fn init(args: &Args) {
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

pub struct Monitor {
    verbose: bool,
    connections: RwLock<AHashMap<Params, Vec<Arc<Connection>>>>,
    descriptors: RwLock<AHashMap<Params, String>>,
    channel: Channel<Params>,
    shutdown_ctl: DuplexChannel<()>,
}

impl Default for Monitor {
    fn default() -> Self {
        Self {
            verbose: false,
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
            .field("verbose", &self.verbose)
            .field("connections", &self.connections)
            .field("descriptors", &self.descriptors)
            .finish()
    }
}

impl Monitor {
    pub fn new(args: &Args) -> Self {
        Self { verbose: args.verbose, ..Default::default() }
    }

    pub fn connections(&self) -> AHashMap<Params, Vec<Arc<Connection>>> {
        self.connections.read().unwrap().clone()
    }

    pub async fn update_nodes(&self, nodes: Vec<Arc<Node>>) -> Result<()> {
        let mut connections = self.connections();

        for params in Params::iter() {
            let nodes = nodes.iter().filter(|node| node.params() == params).collect::<Vec<_>>();

            let list = connections.entry(params).or_default();

            let create: Vec<_> = nodes.iter().filter(|node| !list.iter().any(|connection| connection.node == ***node)).collect();

            let remove: Vec<_> =
                list.iter().filter(|connection| !nodes.iter().any(|node| connection.node == **node)).cloned().collect();

            for node in create {
                let created = Arc::new(Connection::try_new((*node).clone(), self.channel.sender.clone(), self.verbose)?);
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
        Params::iter().for_each(|param| self.channel.sender.try_send(param).unwrap());

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


                            let mut connections = self.connections().get(&params).unwrap().clone();
                            if connections.is_empty() {
                                self.descriptors.write().unwrap().remove(&params);
                            } else {
                                connections.sort_by_key(|connection| connection.score());
                                let output = connections.first().unwrap().output();
                                if self.verbose {
                                    log_success!("Updating","{params} - {output}");
                                }
                                self.descriptors.write().unwrap().insert(params,output);
                            }
                                                    }
                        Err(err) => {
                            println!("Monitor: error while receiving update message: {err}");
                            // break;
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

    pub fn get_all(&self) -> String {
        let connections = self.connections();
        let connections = connections.values().fold(Vec::default(), |mut a, b| {
            a.extend(b);
            a
        });
        // let nodes =
        //     connections.iter().filter_map(|connection| connection.online().then_some(Status::from(*connection))).collect::<Vec<_>>();
        let nodes = connections.iter().map(|connection| Status::from(*connection)).collect::<Vec<_>>();
        serde_json::to_string(&nodes).unwrap()
    }

    pub fn get(&self, params: &Params) -> Option<String> {
        self.descriptors.read().unwrap().get(params).cloned()
    }
}

#[derive(Serialize)]
pub struct Status<'a> {
    pub id: &'a str,
    pub url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<&'a str>,
    pub transport: Transport,
    pub encoding: WrpcEncoding,
    pub network: NetworkId,
    pub online: bool,
    pub status: &'static str,
}

impl<'a> From<&'a Arc<Connection>> for Status<'a> {
    fn from(connection: &'a Arc<Connection>) -> Self {
        let url = connection.node.address.as_str();
        let provider = connection.node.provider.as_deref();
        let link = connection.node.link.as_deref();
        let id = connection.node.id.as_str();
        let transport = connection.node.transport;
        let encoding = connection.node.encoding;
        let network = connection.node.network;
        let status = connection.status();
        let online = connection.online();
        Self { id, url, provider, link, transport, encoding, network, status, online }
    }
}
