use crate::imports::*;
use kaspa_notify::notification::Notification as NotificationT;
pub use kaspa_rpc_macros::{build_wrpc_wasm_bindgen_interface, build_wrpc_wasm_bindgen_subscriptions};
pub use serde_wasm_bindgen::*;

type JsResult<T> = std::result::Result<T, JsError>;

struct NotificationSink(Function);
unsafe impl Send for NotificationSink {}
impl From<NotificationSink> for Function {
    fn from(f: NotificationSink) -> Self {
        f.0
    }
}

/// Kaspa RPC client
#[wasm_bindgen]
pub struct RpcClient {
    client: KaspaRpcClient,
    notification_task: AtomicBool,
    notification_ctl: DuplexChannel,
    notification_callback: Arc<Mutex<Option<NotificationSink>>>,
}

#[wasm_bindgen]
impl RpcClient {
    /// Create a new RPC client with [`Encoding`] and a `url`.
    #[wasm_bindgen(constructor)]
    pub fn new(encoding: Encoding, url: &str) -> RpcClient {
        RpcClient {
            client: KaspaRpcClient::new(encoding, url).unwrap_or_else(|err| panic!("{err}")),
            notification_task: AtomicBool::new(false),
            notification_ctl: DuplexChannel::oneshot(),
            notification_callback: Arc::new(Mutex::new(None)),
        }
    }

    /// Connect to the Kaspa RPC server. This function starts a background
    /// task that connects and reconnects to the server if the connection
    /// is terminated.  Use [`disconnect()`] to terminate the connection.
    pub async fn connect(&self) -> JsResult<()> {
        self.notification_task()?;
        self.client.start().await?;
        self.client.connect(true).await?; //.unwrap();
        Ok(())
    }

    /// Disconnect from the Kaspa RPC server.
    pub async fn disconnect(&self) -> JsResult<()> {
        self.clear_notification_callback();
        self.stop_notification_task().await?;
        self.client.stop().await?;
        self.client.shutdown().await?;
        Ok(())
    }

    async fn stop_notification_task(&self) -> JsResult<()> {
        if self.notification_task.load(Ordering::SeqCst) {
            self.notification_task.store(false, Ordering::SeqCst);
            self.notification_ctl.signal(()).await.map_err(|err| JsError::new(&err.to_string()))?;
        }
        Ok(())
    }

    fn clear_notification_callback(&self) {
        *self.notification_callback.lock().unwrap() = None;
    }

    /// Register a notification callback.
    pub async fn notify(&self, callback: JsValue) -> JsResult<()> {
        if callback.is_function() {
            let fn_callback: Function = callback.into();
            self.notification_callback.lock().unwrap().replace(NotificationSink(fn_callback));
        } else {
            self.stop_notification_task().await?;
            self.clear_notification_callback();
        }
        Ok(())
    }
}

impl RpcClient {
    /// Notification task receives notifications and executes them on the
    /// user-supplied callback function.
    fn notification_task(&self) -> JsResult<()> {
        let ctl_receiver = self.notification_ctl.request.receiver.clone();
        let ctl_sender = self.notification_ctl.response.sender.clone();
        let notification_receiver = self.client.notification_channel_receiver();
        let notification_callback = self.notification_callback.clone();

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
    pub async fn subscribe_daa_score(&self) -> JsResult<()> {
        self.client.start_notify(ListenerId::default(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    /// Unsubscribe from DAA Score (test)
    #[wasm_bindgen(js_name = unsubscribeDaaScore)]
    pub async fn unsubscribe_daa_score(&self) -> JsResult<()> {
        self.client.stop_notify(ListenerId::default(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    // scope variant with field functions

    #[wasm_bindgen(js_name = subscribeVirtualChainChanged)]
    pub async fn subscribe_virtual_chain_changed(&self, include_accepted_transaction_ids: bool) -> JsResult<()> {
        self.client
            .start_notify(
                ListenerId::default(),
                Scope::VirtualChainChanged(VirtualChainChangedScope { include_accepted_transaction_ids }),
            )
            .await?;
        Ok(())
    }
    #[wasm_bindgen(js_name = unsubscribeVirtualChainChanged)]
    pub async fn unsubscribe_virtual_chain_changed(&self, include_accepted_transaction_ids: bool) -> JsResult<()> {
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

#[wasm_bindgen]
struct RpcClient1 {}

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
        GetProcessMetrics,
        GetSelectedTipHash,
        GetSinkBlueScore,
        Ping,
        Shutdown,
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
        GetUtxosByAddresses,
        GetVirtualChainFromBlock,
        ResolveFinalityConflict,
        SubmitBlock,
        SubmitTransaction,
        Unban,
    ]
);
