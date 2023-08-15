use super::result::Result;
use crate::imports::*;
use js_sys::Array;
use kaspa_addresses::Address;
use kaspa_notify::notification::Notification as NotificationT;
pub use kaspa_rpc_macros::{build_wrpc_wasm_bindgen_interface, build_wrpc_wasm_bindgen_subscriptions};
pub use serde_wasm_bindgen::*;

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
    pub fn new(encoding: Encoding, url: &str) -> RpcClient {
        RpcClient {
            client: Arc::new(KaspaRpcClient::new(encoding, url).unwrap_or_else(|err| panic!("{err}"))),
            inner: Arc::new(Inner {
                notification_task: AtomicBool::new(false),
                notification_ctl: DuplexChannel::oneshot(),
                notification_callback: Arc::new(Mutex::new(None)),
            }),
        }
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
        // self.client.start().await?;
        self.client.connect(options).await?; //.unwrap();
        Ok(())
    }

    /// Disconnect from the Kaspa RPC server.
    pub async fn disconnect(&self) -> Result<()> {
        self.clear_notification_callback();
        self.stop_notification_task().await?;
        // self.client.stop().await?;
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
    // experimental/test functions

    /// Subscription to DAA Score (test)
    #[wasm_bindgen(js_name = subscribeDaaScore)]
    pub async fn subscribe_daa_score(&self) -> Result<()> {
        self.client.start_notify(ListenerId::default(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    /// Unsubscribe from DAA Score (test)
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
    pub async fn js_submit_transaction(&self, request: JsValue) -> Result<JsValue> {
        log_info!("submit_transaction req: {:?}", request);
        let response =
            self.submit_transaction(from_value(request)?).await.map_err(|err| wasm_bindgen::JsError::new(&err.to_string()))?;
        to_value(&response).map_err(|err| err.into())
    }

    #[wasm_bindgen(js_name = getUtxosByAddresses)]
    pub async fn get_utxos_by_addresses(&self, request: JsValue) -> Result<JsValue> {
        log_info!("get_utxos_by_addresses req: {:?}", request);
        let request: GetUtxosByAddressesRequest = from_value(request)?;
        //log_info!("get_utxos_by_addresses request: {:?}", request);
        let result: RpcResult<GetUtxosByAddressesResponse> = self.client.get_utxos_by_addresses_call(request).await;
        //log_info!("get_utxos_by_addresses result: {:?}", result);
        let response: GetUtxosByAddressesResponse = result.map_err(|err| wasm_bindgen::JsError::new(&err.to_string()))?;
        //log_info!("get_utxos_by_addresses resp: {:?}", response);
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
