use crate::imports::*;
pub use kaspa_rpc_macros::build_wrpc_wasm_bindgen_interface;
pub use serde_wasm_bindgen::*;

type JsResult<T> = std::result::Result<T, JsError>;

struct NotificationSink(Function);
unsafe impl Send for NotificationSink {}
impl From<NotificationSink> for Function {
    fn from(f: NotificationSink) -> Self {
        f.0
    }
}

#[wasm_bindgen]
pub struct RpcClient {
    client: KaspaRpcClient,
    notification_task: AtomicBool,
    notification_ctl: DuplexChannel,
    notification_callback: Arc<Mutex<Option<NotificationSink>>>,
}

#[wasm_bindgen]
impl RpcClient {
    #[wasm_bindgen(constructor)]
    pub fn new(encoding: Encoding, url: &str) -> RpcClient {
        RpcClient {
            client: KaspaRpcClient::new(encoding, url).unwrap_or_else(|err| panic!("{err}")),
            notification_task: AtomicBool::new(false),
            notification_ctl: DuplexChannel::oneshot(),
            notification_callback: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn connect(&self) -> JsResult<()> {
        self.notification_task()?;
        self.client.start().await?;
        self.client.connect(true).await?; //.unwrap();
        Ok(())
    }

    pub async fn disconnect(&self) -> JsResult<()> {
        if self.notification_task.load(Ordering::SeqCst) {
            self.notification_task.store(false, Ordering::SeqCst);
            self.notification_ctl.signal(()).await.map_err(|err| JsError::new(&err.to_string()))?;
        }
        self.client.stop().await?;
        self.client.shutdown().await?;
        Ok(())
    }

    pub fn notify(&self, callback: Function) -> JsResult<()> {
        self.notification_callback.lock().unwrap().replace(NotificationSink(callback));
        Ok(())
    }
}

impl RpcClient {
    fn notification_task(&self) -> JsResult<()> {
        let ctl_receiver = self.notification_ctl.request.receiver.clone();
        let ctl_sender = self.notification_ctl.response.sender.clone();
        let notification_receiver = self.client.notification_receiver();
        let notification_callback = self.notification_callback.clone();

        spawn(async move {
            loop {
                select! {
                    _ = ctl_receiver.recv().fuse() => {
                        break;
                    },
                    msg = notification_receiver.recv().fuse() => {
                        if let Ok(notification) = &msg {
                            if let Some(callback) = notification_callback.lock().unwrap().as_ref() {
                                let op: RpcApiOps = notification.into();
                                let op_value = to_value(&op).map_err(|err|{
                                    log_error!("Notification handler - unable to convert notification op: {}",err.to_string());
                                }).ok();
                                let op_payload = to_value(&notification).map_err(|err| {
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
        GetVirtualSelectedParentBlueScore,
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
        GetVirtualSelectedParentChainFromBlock,
        ResolveFinalityConflict,
        SubmitBlock,
        SubmitTransaction,
        Unban,
    ]
);
