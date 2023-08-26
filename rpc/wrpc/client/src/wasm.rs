use crate::error::Error;
use crate::imports::*;
use crate::result::Result;
use js_sys::Array;
use kaspa_addresses::{Address, AddressList};
use kaspa_consensus_core::network::{wasm::Network, NetworkType};
use kaspa_consensus_wasm::{SignableTransaction, Transaction};
use kaspa_notify::notification::Notification as NotificationT;
pub use kaspa_rpc_macros::{build_wrpc_wasm_bindgen_interface, build_wrpc_wasm_bindgen_subscriptions};
pub use serde_wasm_bindgen::from_value;
pub use workflow_wasm::serde::to_value;

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
#[wasm_bindgen(inspectable)]
#[derive(Clone)]
pub struct RpcClient {
    #[wasm_bindgen(skip)]
    pub client: Arc<KaspaRpcClient>,
    pub(crate) inner: Arc<Inner>,
}

#[wasm_bindgen]
impl RpcClient {
    /// Create a new RPC client with [`Encoding`] and a `url`.
    #[wasm_bindgen(constructor)]
    pub fn new(url: &str, encoding: Encoding, network_type: Option<Network>) -> Result<RpcClient> {
        let url = if let Some(network_type) = network_type { Self::parse_url(url, encoding, network_type)? } else { url.to_string() };

        let rpc_client = RpcClient {
            client: Arc::new(KaspaRpcClient::new(encoding, url.as_str()).unwrap_or_else(|err| panic!("{err}"))),
            inner: Arc::new(Inner {
                notification_task: AtomicBool::new(false),
                notification_ctl: DuplexChannel::oneshot(),
                notification_callback: Arc::new(Mutex::new(None)),
            }),
        };

        Ok(rpc_client)
    }

    #[wasm_bindgen(getter)]
    pub fn url(&self) -> String {
        self.client.url()
    }

    #[wasm_bindgen(getter, js_name = "open")]
    pub fn is_open(&self) -> bool {
        self.client.is_open()
    }

    /// Connect to the Kaspa RPC server. This function starts a background
    /// task that connects and reconnects to the server if the connection
    /// is terminated.  Use [`disconnect()`] to terminate the connection.
    pub async fn connect(&self, args: JsValue) -> Result<()> {
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
    pub fn parse_url(url: &str, encoding: Encoding, network: Network) -> Result<String> {
        let url_ = KaspaRpcClient::parse_url(Some(url.to_string()), encoding, network.try_into()?)?;
        let url_ = url_.ok_or(Error::custom(format!("received a malformed URL: {url}")))?;
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
    pub async fn subscribe_utxos_changed(&self, addresses: &JsValue) -> Result<()> {
        let addresses = Array::from(addresses)
            .to_vec()
            .into_iter()
            .map(|jsv| from_value(jsv).map_err(|err| JsError::new(&err.to_string())))
            .collect::<std::result::Result<Vec<Address>, JsError>>()?;
        self.client.start_notify(ListenerId::default(), Scope::UtxosChanged(UtxosChangedScope { addresses })).await?;
        Ok(())
    }

    /// Unsubscribe from DAA Score (test)
    #[wasm_bindgen(js_name = unsubscribeUtxosChanged)]
    pub async fn unsubscribe_utxos_changed(&self, addresses: &JsValue) -> Result<()> {
        let addresses = Array::from(addresses)
            .to_vec()
            .into_iter()
            .map(|jsv| from_value(jsv).map_err(|err| JsError::new(&err.to_string())))
            .collect::<std::result::Result<Vec<Address>, JsError>>()?;
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

    // #[wasm_bindgen(js_name = subscribeUtxosChanged)]
    // pub async fn subscribe_utxos_changed(&self, addresses: Vec<Address>) -> JsResult<()> {
    //     self.client.start_notify(ListenerId::default(), Scope::UtxosChanged(UtxosChangedScope { addresses })).await?;
    //     Ok(())
    // }
    // #[wasm_bindgen(js_name = unsubscribeUtxosChanged)]
    // pub async fn unsubscribe_utxos_changed(&self, addresses: Vec<Address>) -> JsResult<()> {
    //     self.client.stop_notify(ListenerId::default(), Scope::UtxosChanged(UtxosChangedScope { addresses })).await?;
    //     Ok(())
    // }
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
// functions that have single arguments (request).

build_wrpc_wasm_bindgen_interface!(
    [
        // functions with no arguments
        GetBlockCount,
        GetBlockDagInfo,
        GetCoinSupply,
        GetConnectedPeerInfo,
        GetInfo,
        GetPeerAddresses,
        GetMetrics,
        GetSelectedTipHash,
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
        GetCurrentNetwork,
        GetHeaders,
        GetMempoolEntries,
        GetMempoolEntriesByAddresses,
        GetMempoolEntry,
        GetSubnetwork,
        // GetUtxosByAddresses,
        GetVirtualChainFromBlock,
        ResolveFinalityConflict,
        SubmitBlock,
        // SubmitTransaction,
        Unban,
    ]
);

#[wasm_bindgen]
impl RpcClient {
    #[wasm_bindgen(js_name = submitTransaction)]
    pub async fn js_submit_transaction(&self, js_value: JsValue, allow_orphan: Option<bool>) -> Result<JsValue> {
        let transaction = if let Ok(signable) = SignableTransaction::try_from(&js_value) {
            Transaction::from(signable)
        } else if let Ok(transaction) = Transaction::try_from(js_value) {
            transaction
        } else {
            return Err(Error::custom("invalid transaction data"));
        };

        let transaction = RpcTransaction::from(transaction);

        let request = SubmitTransactionRequest { transaction, allow_orphan: allow_orphan.unwrap_or(false) };

        // log_info!("submit_transaction req: {:?}", request);
        let response = self.submit_transaction(request).await.map_err(|err| wasm_bindgen::JsError::new(&err.to_string()))?;
        to_value(&response).map_err(|err| err.into())
    }

    /// This call accepts an `Array` of `Address` or an Array of address strings.
    #[wasm_bindgen(js_name = getUtxosByAddresses)]
    pub async fn get_utxos_by_addresses(&self, request: JsValue) -> Result<JsValue> {
        let request = if let Ok(addresses) = AddressList::try_from(&request) {
            GetUtxosByAddressesRequest { addresses: addresses.into() }
        } else {
            from_value::<GetUtxosByAddressesRequest>(request)?
        };

        let result: RpcResult<GetUtxosByAddressesResponse> = self.client.get_utxos_by_addresses_call(request).await;
        let response: GetUtxosByAddressesResponse = result.map_err(|err| wasm_bindgen::JsError::new(&err.to_string()))?;
        to_value(&response.entries).map_err(|err| err.into())
    }

    #[wasm_bindgen(js_name = getUtxosByAddressesCall)]
    pub async fn get_utxos_by_addresses_call(&self, request: JsValue) -> Result<JsValue> {
        let request = from_value::<GetUtxosByAddressesRequest>(request)?;
        let result: RpcResult<GetUtxosByAddressesResponse> = self.client.get_utxos_by_addresses_call(request).await;
        let response: GetUtxosByAddressesResponse = result.map_err(|err| wasm_bindgen::JsError::new(&err.to_string()))?;
        to_value(&response).map_err(|err| err.into())
    }
}

impl RpcClient {
    pub async fn submit_transaction(&self, request: SubmitTransactionRequest) -> Result<SubmitTransactionResponse> {
        let result: RpcResult<SubmitTransactionResponse> = self.client.submit_transaction_call(request).await;
        let response: SubmitTransactionResponse = result?;
        Ok(response)
    }
}
