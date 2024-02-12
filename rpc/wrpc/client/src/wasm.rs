#![allow(non_snake_case)]

use crate::error::Error;
use crate::imports::*;
use crate::result::Result;
// use js_sys::Array;
// use kaspa_addresses::{Address, AddressList};
use kaspa_addresses::{Address, IAddressArray};
use kaspa_consensus_core::network::{wasm::Network, NetworkType};
// use kaspa_consensus_wasm::{SignableTransaction, Transaction};
use kaspa_notify::notification::Notification as NotificationT;
pub use kaspa_rpc_core::wasm::message::*;
pub use kaspa_rpc_macros::{
    build_wrpc_wasm_bindgen_interface, build_wrpc_wasm_bindgen_subscriptions, declare_typescript_wasm_interface as declare,
};
pub use serde_wasm_bindgen::from_value;
pub use workflow_rpc::client::IConnectOptions;
use workflow_wasm::extensions::ObjectExtension;
pub use workflow_wasm::serde::to_value;

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
         * URL for wRPC node endpoint
         */
        url?: string;
        /**
         * RPC encoding: `borsh` (default) or `json`
         */
        encoding?: Encoding;
        /**
         * Network identifier: `mainnet` or `testnet-10`
         */
        network?: NetworkId;
    }
    "#,
}

pub struct RpcConfig {
    pub url: Option<String>,
    pub encoding: Encoding,
    pub network_id: Option<NetworkId>,
}

impl Default for RpcConfig {
    fn default() -> Self {
        RpcConfig { url: None, encoding: Encoding::Borsh, network_id: None }
    }
}

impl TryFrom<IRpcConfig> for RpcConfig {
    type Error = Error;
    fn try_from(config: IRpcConfig) -> Result<Self> {
        let url = config.try_get_string("url")?; //.ok_or_else(|| Error::custom("url is required"))?;
        let encoding = config.try_get::<Encoding>("encoding")?.unwrap_or(Encoding::Borsh); //map_or(Ok(Encoding::Borsh), |encoding| Encoding::try_from(encoding))?;
        let network_id = config.try_get::<NetworkId>("network")?.ok_or_else(|| Error::custom("network is required"))?;
        Ok(RpcConfig { url, encoding, network_id: Some(network_id) })
    }
}

impl TryFrom<RpcConfig> for IRpcConfig {
    type Error = Error;
    fn try_from(config: RpcConfig) -> Result<Self> {
        let object = IRpcConfig::default();
        object.set("url", &config.url.into())?;
        object.set("encoding", &config.encoding.into())?;
        object.set("network", &config.network_id.into())?;
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
    #[wasm_bindgen(skip)]
    pub client: Arc<KaspaRpcClient>,
    pub(crate) inner: Arc<Inner>,
}

impl RpcClient {
    pub fn new(config: Option<RpcConfig>) -> Result<RpcClient> {
        let RpcConfig { url, encoding, network_id } = config.unwrap_or_default();

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

        let rpc_client = RpcClient {
            client: Arc::new(KaspaRpcClient::new(encoding, url.as_deref()).unwrap_or_else(|err| panic!("{err}"))),
            inner: Arc::new(Inner {
                notification_task: AtomicBool::new(false),
                notification_ctl: DuplexChannel::oneshot(),
                notification_callback: Arc::new(Mutex::new(None)),
            }),
        };

        Ok(rpc_client)
    }
}

#[wasm_bindgen]
impl RpcClient {
    /// Create a new RPC client with [`Encoding`] and a `url`.
    #[wasm_bindgen(constructor)]
    pub fn ctor(config: Option<IRpcConfig>) -> Result<RpcClient> {
        Self::new(config.map(RpcConfig::try_from).transpose()?)
    }

    #[wasm_bindgen(getter)]
    pub fn url(&self) -> Option<String> {
        self.client.url()
    }

    #[wasm_bindgen(getter, js_name = "open")]
    pub fn is_open(&self) -> bool {
        self.client.is_open()
    }

    /// Connect to the Kaspa RPC server. This function starts a background
    /// task that connects and reconnects to the server if the connection
    /// is terminated.  Use [`disconnect()`](Self::disconnect()) to
    /// terminate the connection.
    pub async fn connect(&self, args: &IConnectOptions) -> Result<()> {
        let options: ConnectOptions = args.try_into()?;

        self.start_notification_task()?;
        self.client.connect(options).await?;
        Ok(())
    }

    /// Disconnect from the Kaspa RPC server.
    pub async fn disconnect(&self) -> Result<()> {
        self.clear_notification_callback();
        self.stop_notification_task().await?;
        self.client.shutdown().await?;
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
    pub async fn notify(&self, callback: JsValue) -> Result<()> {
        if callback.is_function() {
            let fn_callback: Function = callback.into();
            self.inner.notification_callback.lock().unwrap().replace(NotificationSink(fn_callback));
        } else {
            self.stop_notification_task().await?;
            self.clear_notification_callback();
        }
        Ok(())
    }
}

impl RpcClient {
    pub fn new_with_rpc_client(client: Arc<KaspaRpcClient>) -> RpcClient {
        RpcClient {
            client,
            inner: Arc::new(Inner {
                notification_task: AtomicBool::new(false),
                notification_ctl: DuplexChannel::oneshot(),
                notification_callback: Arc::new(Mutex::new(None)),
            }),
        }
    }

    pub fn client(&self) -> &Arc<KaspaRpcClient> {
        &self.client
    }

    /// Notification task receives notifications and executes them on the
    /// user-supplied callback function.
    fn start_notification_task(&self) -> Result<()> {
        let ctl_receiver = self.inner.notification_ctl.request.receiver.clone();
        let ctl_sender = self.inner.notification_ctl.response.sender.clone();
        let notification_receiver = self.client.notification_channel_receiver();
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
    /// Subscription to DAA Score
    #[wasm_bindgen(js_name = subscribeDaaScore)]
    pub async fn subscribe_daa_score(&self) -> Result<()> {
        self.client.start_notify(ListenerId::default(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    /// Unsubscribe from DAA Score
    #[wasm_bindgen(js_name = unsubscribeDaaScore)]
    pub async fn unsubscribe_daa_score(&self) -> Result<()> {
        self.client.stop_notify(ListenerId::default(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    /// Subscription to UTXOs Changed notifications
    #[wasm_bindgen(js_name = subscribeUtxosChanged)]
    pub async fn subscribe_utxos_changed(&self, addresses: IAddressArray) -> Result<()> {
        let addresses: Vec<Address> = addresses.try_into()?;
        self.client.start_notify(ListenerId::default(), Scope::UtxosChanged(UtxosChangedScope { addresses })).await?;
        Ok(())
    }

    /// Unsubscribe from DAA Score (test)
    #[wasm_bindgen(js_name = unsubscribeUtxosChanged)]
    pub async fn unsubscribe_utxos_changed(&self, addresses: IAddressArray) -> Result<()> {
        let addresses: Vec<Address> = addresses.try_into()?;
        self.client.stop_notify(ListenerId::default(), Scope::UtxosChanged(UtxosChangedScope { addresses })).await?;
        Ok(())
    }

    // scope variant with field functions
    #[wasm_bindgen(js_name = subscribeVirtualChainChanged)]
    pub async fn subscribe_virtual_chain_changed(&self, include_accepted_transaction_ids: bool) -> Result<()> {
        self.client
            .start_notify(
                ListenerId::default(),
                Scope::VirtualChainChanged(VirtualChainChangedScope { include_accepted_transaction_ids }),
            )
            .await?;
        Ok(())
    }
    #[wasm_bindgen(js_name = unsubscribeVirtualChainChanged)]
    pub async fn unsubscribe_virtual_chain_changed(&self, include_accepted_transaction_ids: bool) -> Result<()> {
        self.client
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
    BlockAdded,
    //VirtualChainChanged, // can't used this here due to non-C-style enum variant
    FinalityConflict,
    FinalityConflictResolved,
    //UtxosChanged, // can't used this here due to non-C-style enum variant
    SinkBlueScoreChanged,
    VirtualDaaScoreChanged,
    PruningPointUtxoSetOverride,
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
        GetBlockCount,
        GetBlockDagInfo,
        GetCoinSupply,
        GetConnectedPeerInfo,
        GetInfo,
        GetPeerAddresses,
        GetMetrics,
        GetSink,
        GetSinkBlueScore,
        Ping,
        Shutdown,
        GetServerInfo,
        GetSyncStatus,
    ],
    [
        // functions with `request` argument
        AddPeer,
        Ban,
        EstimateNetworkHashesPerSecond,
        GetBalanceByAddress,
        GetBalancesByAddresses,
        GetBlock,
        GetBlocks,
        GetBlockTemplate,
        GetDaaScoreTimestampEstimate,
        GetCurrentNetwork,
        GetHeaders,
        GetMempoolEntries,
        GetMempoolEntriesByAddresses,
        GetMempoolEntry,
        GetSubnetwork,
        GetUtxosByAddresses,
        GetVirtualChainFromBlock,
        ResolveFinalityConflict,
        SubmitBlock,
        SubmitTransaction,
        Unban,
    ]
);
