#![allow(non_snake_case)]

use crate::imports::*;
use crate::Resolver;
use crate::RpcNotificationCallback;
use kaspa_addresses::{Address, IAddressArray};
use kaspa_consensus_core::network::{wasm::Network, NetworkType};
use kaspa_notify::notification::Notification as NotificationT;
pub use kaspa_rpc_core::wasm::message::*;
pub use kaspa_rpc_macros::{
    build_wrpc_wasm_bindgen_interface, build_wrpc_wasm_bindgen_subscriptions, declare_typescript_wasm_interface as declare,
};
pub use serde_wasm_bindgen::from_value;
pub use workflow_rpc::encoding::Encoding as WrpcEncoding;
use workflow_wasm::extensions::ObjectExtension;
pub use workflow_wasm::serde::to_value;

#[wasm_bindgen(typescript_custom_section)]
const TS_CONNECT_OPTIONS: &'static str = r#"

/**
 * `ConnectOptions` is used to configure the `WebSocket` connectivity behavior.
 * 
 * @category WebSocket
 */
export interface IConnectOptions {
    /**
     * Indicates if the `async fn connect()` method should return immediately
     * or wait for connection to occur or fail before returning.
     * (default is `true`)
     */
    blockAsyncConnect? : boolean,
    /**
     * ConnectStrategy used to configure the retry or fallback behavior.
     * In retry mode, the WebSocket will continuously attempt to connect to the server.
     * (default is {link ConnectStrategy.Retry}).
     */
    strategy?: ConnectStrategy | string,
    /** 
     * A custom URL that will change the current URL of the WebSocket.
     * If supplied, the URL will override the resolver.
     */
    url?: string,
    /**
     * A custom connection timeout in milliseconds.
     */
    timeoutDuration?: number,
    /** 
     * A custom retry interval in milliseconds.
     */
    retryInterval?: number,
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "IConnectOptions | undefined")]
    pub type IConnectOptions;
}

impl From<IConnectOptions> for workflow_rpc::client::IConnectOptions {
    fn from(options: IConnectOptions) -> Self {
        options.into()
    }
}

declare! {
    IRpcConfig,
    r#"
    /**
     * RPC client configuration options
     * 
     * @category Node RPC
     */
    export interface IRpcConfig {
        /**
         * An instance of the {@link Resolver} class to use for an automatic public node lookup.
         * If supplying a resolver, the `url` property is ignored.
         */
        resolver? : Resolver,
        /**
         * URL for wRPC node endpoint
         */
        url?: string;
        /**
         * RPC encoding: `borsh` or `json` (default is `borsh`)
         */
        encoding?: Encoding;
        /**
         * Network identifier: `mainnet` or `testnet-10`
         */
        networkId?: NetworkId;
    }
    "#,
}

pub struct RpcConfig {
    pub resolver: Option<Resolver>,
    pub url: Option<String>,
    pub encoding: Option<Encoding>,
    pub network_id: Option<NetworkId>,
}

impl Default for RpcConfig {
    fn default() -> Self {
        RpcConfig { url: None, encoding: Some(Encoding::Borsh), network_id: None, resolver: None }
    }
}

impl TryFrom<IRpcConfig> for RpcConfig {
    type Error = Error;
    fn try_from(config: IRpcConfig) -> Result<Self> {
        let resolver = config.try_get::<Resolver>("resolver")?;
        let url = config.try_get_string("url")?;
        let encoding = config.try_get::<Encoding>("encoding")?; //.unwrap_or(Encoding::Borsh); //map_or(Ok(Encoding::Borsh), |encoding| Encoding::try_from(encoding))?;
        let network_id = config.try_get::<NetworkId>("networkId")?.ok_or_else(|| Error::custom("network is required"))?;
        Ok(RpcConfig { resolver, url, encoding, network_id: Some(network_id) })
    }
}

impl TryFrom<RpcConfig> for IRpcConfig {
    type Error = Error;
    fn try_from(config: RpcConfig) -> Result<Self> {
        let object = IRpcConfig::default();
        object.set("resolver", &config.resolver.into())?;
        object.set("url", &config.url.into())?;
        object.set("encoding", &config.encoding.into())?;
        object.set("networkId", &config.network_id.into())?;
        Ok(object)
    }
}

struct NotificationSink(Function);
unsafe impl Send for NotificationSink {}
impl From<NotificationSink> for Function {
    fn from(f: NotificationSink) -> Self {
        f.0
    }
}

pub struct Inner {
    client: Arc<KaspaRpcClient>,
    notification_task: AtomicBool,
    notification_ctl: DuplexChannel,
    notification_callback: Arc<Mutex<Option<NotificationSink>>>,
}

/// Kaspa RPC client
///
/// Kaspa RPC client uses wRPC (Workflow RPC) WebSocket interface to connect
/// to directly to Kaspa Node.
///
/// @category Node RPC
#[wasm_bindgen(inspectable)]
#[derive(Clone)]
pub struct RpcClient {
    // #[wasm_bindgen(skip)]
    pub(crate) inner: Arc<Inner>,
    resolver: Option<Resolver>,
}

impl RpcClient {
    pub fn new(config: Option<RpcConfig>) -> Result<RpcClient> {
        let RpcConfig { resolver, url, encoding, network_id } = config.unwrap_or_default();

        let encoding = encoding.unwrap_or(Encoding::Borsh);

        let url = url
            .map(
                |url| {
                    if let Some(network_id) = network_id {
                        Self::parse_url(&url, encoding, network_id)
                    } else {
                        Ok(url.to_string())
                    }
                },
            )
            .transpose()?;

        let client = Arc::new(
            KaspaRpcClient::new(encoding, url.as_deref(), resolver.clone().map(Into::into), network_id)
                .unwrap_or_else(|err| panic!("{err}")),
        );

        let rpc_client = RpcClient {
            inner: Arc::new(Inner {
                client,
                notification_task: AtomicBool::new(false),
                notification_ctl: DuplexChannel::oneshot(),
                notification_callback: Arc::new(Mutex::new(None)),
            }),
            resolver,
        };

        Ok(rpc_client)
    }
}

#[wasm_bindgen]
impl RpcClient {
    /// Create a new RPC client with optional [`Encoding`] and a `url`.
    /// @see {@link IRpcConfig} interface for more details.
    #[wasm_bindgen(constructor)]
    pub fn ctor(config: Option<IRpcConfig>) -> Result<RpcClient> {
        Self::new(config.map(RpcConfig::try_from).transpose()?)
    }

    /// The current URL of the RPC client.
    #[wasm_bindgen(getter)]
    pub fn url(&self) -> Option<String> {
        self.inner.client.url()
    }

    /// Current rpc resolver
    #[wasm_bindgen(getter)]
    pub fn resolver(&self) -> Option<Resolver> {
        self.resolver.clone()
    }

    /// The current connection status of the RPC client.
    #[wasm_bindgen(getter, js_name = "isConnected")]
    pub fn is_connected(&self) -> bool {
        self.inner.client.is_connected()
    }

    /// The current protocol encoding.
    #[wasm_bindgen(getter, js_name = "encoding")]
    pub fn encoding(&self) -> String {
        self.inner.client.encoding().to_string()
    }

    /// Optional: Resolver node id.
    #[wasm_bindgen(getter, js_name = "nodeId")]
    pub fn resolver_node_id(&self) -> Option<String> {
        self.inner.client.node_descriptor().map(|node| node.id.clone())
    }

    /// Optional: public node provider name.
    #[wasm_bindgen(getter, js_name = "providerName")]
    pub fn resolver_node_provider_name(&self) -> Option<String> {
        self.inner.client.node_descriptor().and_then(|node| node.provider_name.clone())
    }

    /// Optional: public node provider URL.
    #[wasm_bindgen(getter, js_name = "providerUrl")]
    pub fn resolver_node_provider_url(&self) -> Option<String> {
        self.inner.client.node_descriptor().and_then(|node| node.provider_url.clone())
    }

    /// Connect to the Kaspa RPC server. This function starts a background
    /// task that connects and reconnects to the server if the connection
    /// is terminated.  Use [`disconnect()`](Self::disconnect()) to
    /// terminate the connection.
    /// @see {@link IConnectOptions} interface for more details.
    pub async fn connect(&self, args: Option<IConnectOptions>) -> Result<()> {
        let args = args.map(workflow_rpc::client::IConnectOptions::from);
        let options = args.map(ConnectOptions::try_from).transpose()?;

        self.start_notification_task()?;
        self.inner.client.connect(options).await?;
        Ok(())
    }

    /// Disconnect from the Kaspa RPC server.
    pub async fn disconnect(&self) -> Result<()> {
        self.clear_notification_callback();
        self.stop_notification_task().await?;
        self.inner.client.shutdown().await?;
        Ok(())
    }

    async fn stop_notification_task(&self) -> Result<()> {
        if self.inner.notification_task.load(Ordering::SeqCst) {
            self.inner.notification_task.store(false, Ordering::SeqCst);
            self.inner.notification_ctl.signal(()).await.map_err(|err| JsError::new(&err.to_string()))?;
        }
        Ok(())
    }

    fn clear_notification_callback(&self) {
        *self.inner.notification_callback.lock().unwrap() = None;
    }

    /// Register a notification callback.
    /// IMPORTANT: You are allowed to register only one callback.
    #[wasm_bindgen(js_name = "registerListener")]
    pub async fn register_event_listener(&self, callback: RpcNotificationCallback) -> Result<()> {
        if callback.is_function() {
            let fn_callback: Function = callback.into();
            self.inner.notification_callback.lock().unwrap().replace(NotificationSink(fn_callback));
        } else {
            self.stop_notification_task().await?;
            self.clear_notification_callback();
        }
        Ok(())
    }

    /// Unregister a notification callback.
    #[wasm_bindgen(js_name = "removeListener")]
    pub async fn remove_event_listener(&self) -> Result<()> {
        self.stop_notification_task().await?;
        self.clear_notification_callback();
        Ok(())
    }
}

impl RpcClient {
    pub fn new_with_rpc_client(client: Arc<KaspaRpcClient>) -> RpcClient {
        let resolver = client.resolver().map(Into::into);
        RpcClient {
            inner: Arc::new(Inner {
                client,
                notification_task: AtomicBool::new(false),
                notification_ctl: DuplexChannel::oneshot(),
                notification_callback: Arc::new(Mutex::new(None)),
            }),
            resolver,
        }
    }

    pub fn client(&self) -> &Arc<KaspaRpcClient> {
        &self.inner.client
    }

    /// Notification task receives notifications and executes them on the
    /// user-supplied callback function.
    fn start_notification_task(&self) -> Result<()> {
        let ctl_receiver = self.inner.notification_ctl.request.receiver.clone();
        let ctl_sender = self.inner.notification_ctl.response.sender.clone();
        let notification_receiver = self.inner.client.notification_channel_receiver();
        let notification_callback = self.inner.notification_callback.clone();

        spawn(async move {
            loop {
                select! {
                    _ = ctl_receiver.recv().fuse() => {
                        break;
                    },
                    msg = notification_receiver.recv().fuse() => {
                        // log_info!("notification: {:?}",msg);
                        if let Ok(notification) = &msg {
                            if let Some(callback) = notification_callback.lock().unwrap().as_ref() {
                                let op: RpcApiOps = notification.event_type().into();
                                let op_value = to_value(&op).map_err(|err|{
                                    log_error!("Notification handler - unable to convert notification op: {}",err.to_string());
                                }).ok();
                                let op_payload = notification.to_value().map_err(|err| {
                                    log_error!("Notification handler - unable to convert notification payload: {}",err.to_string());
                                }).ok();
                                if op_value.is_none() || op_payload.is_none() {
                                    continue;
                                }
                                if let Err(err) = callback.0.call2(&JsValue::undefined(), &op_value.unwrap(), &op_payload.unwrap()) {
                                    log_error!("Error while executing notification callback: {:?}",err);
                                }
                            }
                        }
                    }
                }
            }

            ctl_sender.send(()).await.ok();
        });

        Ok(())
    }
}

#[wasm_bindgen]
impl RpcClient {
    #[wasm_bindgen(js_name = "defaultPort")]
    pub fn default_port(encoding: WrpcEncoding, network: Network) -> Result<u16> {
        let network_type = NetworkType::try_from(network)?;
        match encoding {
            WrpcEncoding::Borsh => Ok(network_type.default_borsh_rpc_port()),
            WrpcEncoding::SerdeJson => Ok(network_type.default_json_rpc_port()),
        }
    }

    /// Constructs an WebSocket RPC URL given the partial URL or an IP, RPC encoding
    /// and a network type.
    ///
    /// # Arguments
    ///
    /// * `url` - Partial URL or an IP address
    /// * `encoding` - RPC encoding
    /// * `network_type` - Network type
    ///
    #[wasm_bindgen(js_name = parseUrl)]
    pub fn parse_url(url: &str, encoding: Encoding, network: NetworkId) -> Result<String> {
        let url_ = KaspaRpcClient::parse_url(url.to_string(), encoding, network.into())?;
        Ok(url_)
    }
}

#[wasm_bindgen]
impl RpcClient {
    // / This call accepts an `Array` of `Address` or an Array of address strings.
    // #[wasm_bindgen(js_name = getUtxosByAddresses)]
    // pub async fn get_utxos_by_addresses(&self, request: IGetUtxosByAddressesRequest) -> Result<GetUtxosByAddressesResponse> {
    //     let request : GetUtxosByAddressesRequest = request.try_into()?;
    //     let result: RpcResult<GetUtxosByAddressesResponse> = self.inner.client.get_utxos_by_addresses_call(request).await;
    //     let response: GetUtxosByAddressesResponse = result.map_err(|err| wasm_bindgen::JsError::new(&err.to_string()))?;
    //     to_value(&response.entries).map_err(|err| err.into())
    // }

    // #[wasm_bindgen(js_name = getUtxosByAddressesCall)]
    // pub async fn get_utxos_by_addresses_call(&self, request: IGetUtxosByAddressesRequest) -> Result<IGetUtxosByAddressesResponse> {
    //     let request = from_value::<GetUtxosByAddressesRequest>(request)?;
    //     let result: RpcResult<GetUtxosByAddressesResponse> = self.inner.client.get_utxos_by_addresses_call(request).await;
    //     let response: GetUtxosByAddressesResponse = result.map_err(|err| wasm_bindgen::JsError::new(&err.to_string()))?;
    //     to_value(&response).map_err(|err| err.into())
    // }

    // ---

    /// Manage subscription for a virtual DAA score changed notification event.
    /// Virtual DAA score changed notification event is produced when the virtual
    /// Difficulty Adjustment Algorithm (DAA) score changes in the Kaspa BlockDAG.
    #[wasm_bindgen(js_name = subscribeDaaScore)]
    pub async fn subscribe_daa_score(&self) -> Result<()> {
        self.inner.client.start_notify(ListenerId::default(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    /// Manage subscription for a virtual DAA score changed notification event.
    /// Virtual DAA score changed notification event is produced when the virtual
    /// Difficulty Adjustment Algorithm (DAA) score changes in the Kaspa BlockDAG.
    #[wasm_bindgen(js_name = unsubscribeDaaScore)]
    pub async fn unsubscribe_daa_score(&self) -> Result<()> {
        self.inner.client.stop_notify(ListenerId::default(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    /// Subscribe for a UTXOs changed notification event.
    /// UTXOs changed notification event is produced when the set
    /// of unspent transaction outputs (UTXOs) changes in the
    /// Kaspa BlockDAG. The event notification will be scoped to the
    /// provided list of addresses.
    #[wasm_bindgen(js_name = subscribeUtxosChanged)]
    pub async fn subscribe_utxos_changed(&self, addresses: IAddressArray) -> Result<()> {
        let addresses: Vec<Address> = addresses.try_into()?;
        self.inner.client.start_notify(ListenerId::default(), Scope::UtxosChanged(UtxosChangedScope { addresses })).await?;
        Ok(())
    }

    /// Unsubscribe from UTXOs changed notification event
    /// for a specific set of addresses.
    #[wasm_bindgen(js_name = unsubscribeUtxosChanged)]
    pub async fn unsubscribe_utxos_changed(&self, addresses: IAddressArray) -> Result<()> {
        let addresses: Vec<Address> = addresses.try_into()?;
        self.inner.client.stop_notify(ListenerId::default(), Scope::UtxosChanged(UtxosChangedScope { addresses })).await?;
        Ok(())
    }

    // TODO: scope variant with field functions

    /// Manage subscription for a virtual chain changed notification event.
    /// Virtual chain changed notification event is produced when the virtual
    /// chain changes in the Kaspa BlockDAG.
    #[wasm_bindgen(js_name = subscribeVirtualChainChanged)]
    pub async fn subscribe_virtual_chain_changed(&self, include_accepted_transaction_ids: bool) -> Result<()> {
        self.inner
            .client
            .start_notify(
                ListenerId::default(),
                Scope::VirtualChainChanged(VirtualChainChangedScope { include_accepted_transaction_ids }),
            )
            .await?;
        Ok(())
    }

    /// Manage subscription for a virtual chain changed notification event.
    /// Virtual chain changed notification event is produced when the virtual
    /// chain changes in the Kaspa BlockDAG.
    #[wasm_bindgen(js_name = unsubscribeVirtualChainChanged)]
    pub async fn unsubscribe_virtual_chain_changed(&self, include_accepted_transaction_ids: bool) -> Result<()> {
        self.inner
            .client
            .stop_notify(
                ListenerId::default(),
                Scope::VirtualChainChanged(VirtualChainChangedScope { include_accepted_transaction_ids }),
            )
            .await?;
        Ok(())
    }
}

// Build subscribe functions
build_wrpc_wasm_bindgen_subscriptions!([
    // Manually implemented subscriptions (above)
    // VirtualChainChanged, // can't used this here due to non-C-style enum variant
    // UtxosChanged, // can't used this here due to non-C-style enum variant
    // VirtualDaaScoreChanged,
    /// Manage subscription for a block added notification event.
    /// Block added notification event is produced when a new
    /// block is added to the Kaspa BlockDAG.
    BlockAdded,
    /// Manage subscription for a finality conflict notification event.
    /// Finality conflict notification event is produced when a finality
    /// conflict occurs in the Kaspa BlockDAG.
    FinalityConflict,
    // TODO provide better description
    /// Manage subscription for a finality conflict resolved notification event.
    /// Finality conflict resolved notification event is produced when a finality
    /// conflict in the Kaspa BlockDAG is resolved.
    FinalityConflictResolved,
    /// Manage subscription for a sink blue score changed notification event.
    /// Sink blue score changed notification event is produced when the blue
    /// score of the sink block changes in the Kaspa BlockDAG.
    SinkBlueScoreChanged,
    /// Manage subscription for a pruning point UTXO set override notification event.
    /// Pruning point UTXO set override notification event is produced when the
    /// UTXO set override for the pruning point changes in the Kaspa BlockDAG.
    PruningPointUtxoSetOverride,
    /// Manage subscription for a new block template notification event.
    /// New block template notification event is produced when a new block
    /// template is generated for mining in the Kaspa BlockDAG.
    NewBlockTemplate,
]);

// Build RPC method invocation functions. This macro
// takes two lists.  First list is for functions that
// do not have arguments and the second one is for
// functions that have a single argument (request).

build_wrpc_wasm_bindgen_interface!(
    [
        // functions with optional arguments
        // they are specified as Option<IXxxRequest>
        // which map as `request? : IXxxRequest` in typescript
        /// Retrieves the current number of blocks in the Kaspa BlockDAG.
        /// This is not a block count, not a "block height" and can not be
        /// used for transaction validation.
        /// Returned information: Current block count.
        GetBlockCount,
        /// Provides information about the Directed Acyclic Graph (DAG)
        /// structure of the Kaspa BlockDAG.
        /// Returned information: Number of blocks in the DAG,
        /// number of tips in the DAG, hash of the selected parent block,
        /// difficulty of the selected parent block, selected parent block
        /// blue score, selected parent block time.
        GetBlockDagInfo,
        /// Returns the total current coin supply of Kaspa network.
        /// Returned information: Total coin supply.
        GetCoinSupply,
        /// Retrieves information about the peers connected to the Kaspa node.
        /// Returned information: Peer ID, IP address and port, connection
        /// status, protocol version.
        GetConnectedPeerInfo,
        /// Retrieves general information about the Kaspa node.
        /// Returned information: Version of the Kaspa node, protocol
        /// version, network identifier.
        /// This call is primarily used by gRPC clients.
        /// For wRPC clients, use {@link RpcClient.getServerInfo}.
        GetInfo,
        /// Provides a list of addresses of known peers in the Kaspa
        /// network that the node can potentially connect to.
        /// Returned information: List of peer addresses.
        GetPeerAddresses,
        /// Retrieves various metrics and statistics related to the
        /// performance and status of the Kaspa node.
        /// Returned information: Memory usage, CPU usage, network activity.
        GetMetrics,
        /// Retrieves the current sink block, which is the block with
        /// the highest cumulative difficulty in the Kaspa BlockDAG.
        /// Returned information: Sink block hash, sink block height.
        GetSink,
        /// Returns the blue score of the current sink block, indicating
        /// the total amount of work that has been done on the main chain
        /// leading up to that block.
        /// Returned information: Blue score of the sink block.
        GetSinkBlueScore,
        /// Tests the connection and responsiveness of a Kaspa node.
        /// Returned information: None.
        Ping,
        /// Gracefully shuts down the Kaspa node.
        /// Returned information: None.
        Shutdown,
        /// Retrieves information about the Kaspa server.
        /// Returned information: Version of the Kaspa server, protocol
        /// version, network identifier.
        GetServerInfo,
        /// Obtains basic information about the synchronization status of the Kaspa node.
        /// Returned information: Syncing status.
        GetSyncStatus,
    ],
    [
        // functions with `request` argument
        /// Adds a peer to the Kaspa node's list of known peers.
        /// Returned information: None.
        AddPeer,
        /// Bans a peer from connecting to the Kaspa node for a specified duration.
        /// Returned information: None.
        Ban,
        /// Estimates the network's current hash rate in hashes per second.
        /// Returned information: Estimated network hashes per second.
        EstimateNetworkHashesPerSecond,
        /// Retrieves the balance of a specific address in the Kaspa BlockDAG.
        /// Returned information: Balance of the address.
        GetBalanceByAddress,
        /// Retrieves balances for multiple addresses in the Kaspa BlockDAG.
        /// Returned information: Balances of the addresses.
        GetBalancesByAddresses,
        /// Retrieves a specific block from the Kaspa BlockDAG.
        /// Returned information: Block information.
        GetBlock,
        /// Retrieves multiple blocks from the Kaspa BlockDAG.
        /// Returned information: List of block information.
        GetBlocks,
        /// Generates a new block template for mining.
        /// Returned information: Block template information.
        GetBlockTemplate,
        /// Retrieves the estimated DAA (Difficulty Adjustment Algorithm)
        /// score timestamp estimate.
        /// Returned information: DAA score timestamp estimate.
        GetDaaScoreTimestampEstimate,
        /// Retrieves the current network configuration.
        /// Returned information: Current network configuration.
        GetCurrentNetwork,
        /// Retrieves block headers from the Kaspa BlockDAG.
        /// Returned information: List of block headers.
        GetHeaders,
        /// Retrieves mempool entries from the Kaspa node's mempool.
        /// Returned information: List of mempool entries.
        GetMempoolEntries,
        /// Retrieves mempool entries associated with specific addresses.
        /// Returned information: List of mempool entries.
        GetMempoolEntriesByAddresses,
        /// Retrieves a specific mempool entry by transaction ID.
        /// Returned information: Mempool entry information.
        GetMempoolEntry,
        /// Retrieves information about a subnetwork in the Kaspa BlockDAG.
        /// Returned information: Subnetwork information.
        GetSubnetwork,
        /// Retrieves unspent transaction outputs (UTXOs) associated with
        /// specific addresses.
        /// Returned information: List of UTXOs.
        GetUtxosByAddresses,
        /// Retrieves the virtual chain corresponding to a specified block hash.
        /// Returned information: Virtual chain information.
        GetVirtualChainFromBlock,
        /// Resolves a finality conflict in the Kaspa BlockDAG.
        /// Returned information: None.
        ResolveFinalityConflict,
        /// Submits a block to the Kaspa network.
        /// Returned information: None.
        SubmitBlock,
        /// Submits a transaction to the Kaspa network.
        /// Returned information: None.
        SubmitTransaction,
        /// Unbans a previously banned peer, allowing it to connect
        /// to the Kaspa node again.
        /// Returned information: None.
        Unban,
    ]
);
