use crate::imports::*;
pub use kaspa_rpc_macros::build_wrpc_client_interface;

#[wasm_bindgen]
#[derive(Clone)]
pub enum NotificationMode {
    // Synced,
    NotSynced,
    Direct,
}

/// [`KaspaRpcClient`] allows connection to the Kaspa wRPC Server via
/// binary Borsh or JSON protocols.
// #[wasm_bindgen]
#[derive(Clone)]
pub struct KaspaRpcClient {
    rpc: Arc<RpcClient<RpcApiOps>>,
    notifier: Option<Arc<Notifier>>,
    notification_mode: NotificationMode,
    // notification_ctl: DuplexChannel,
    notification_channel: Channel<Notification>,
    encoding: Encoding,
}

// #[wasm_bindgen]
impl KaspaRpcClient {
    // #[wasm_bindgen(constructor)]
    pub fn new(encoding: Encoding, url: &str) -> Result<KaspaRpcClient> {
        Self::new_with_args(encoding, NotificationMode::Direct, url)
    }

    pub fn new_with_args(encoding: Encoding, notification_mode: NotificationMode, url: &str) -> Result<KaspaRpcClient> {
        let re = Regex::new(r"^wrpc").unwrap();
        let url = re.replace(url, "ws").to_string();
        // log_trace!("Kaspa wRPC::{encoding} connecting to: {url}");
        let options = RpcClientOptions { url: &url, ..RpcClientOptions::default() };

        let notifier = if matches!(notification_mode, NotificationMode::NotSynced) {
            Some(Arc::new(Notifier::new(None, None, ListenerUtxoNotificationFilterSetting::FilteredByAddress)))
        } else {
            None
        };

        let notification_channel = Channel::unbounded();

        // The `Interface` struct can be used to register for server-side
        // notifications. All notification methods have to be created at
        // this stage.
        let mut interface = Interface::<RpcApiOps>::new();

        [
            RpcApiOps::NotifyBlockAdded,
            RpcApiOps::NotifyFinalityConflict,
            RpcApiOps::NotifyFinalityConflicts,
            RpcApiOps::NotifyNewBlockTemplate,
            RpcApiOps::NotifyPruningPointUtxoSetOverride,
            RpcApiOps::NotifyUtxosChanged,
            RpcApiOps::NotifyVirtualDaaScoreChanged,
            RpcApiOps::NotifyVirtualSelectedParentBlueScoreChanged,
            RpcApiOps::NotifyVirtualSelectedParentChainChanged,
        ]
        .into_iter()
        .for_each(|notification_op| {
            let notifier_ = notifier.clone();
            let notification_sender_ = notification_channel.sender.clone();
            interface.notification(
                notification_op,
                workflow_rpc::client::Notification::new(move |notification: rpc_core::Notification| {
                    let notifier = notifier_.clone();
                    let notification_sender = notification_sender_.clone();
                    Box::pin(async move {
                        // log_info!("notification receivers: {}", notification_sender.receiver_count());
                        // log_trace!("notification {:?}", notification);
                        if let Some(notifier) = &notifier {
                            // log_info!("notification: posting to notifier");
                            let _res = notifier.clone().notify(notification.into());
                            // log_trace!("notifier.notify: result {:?}", _res);
                        } else if notification_sender.receiver_count() > 1 {
                            // log_info!("notification: posting direct");
                            notification_sender.send(notification).await?;
                        } else {
                            log_warning!("WARNING: Kaspa RPC notification is not consumed by user: {:?}", notification);
                        }
                        Ok(())
                    })
                }),
            );
        });

        let client = KaspaRpcClient {
            rpc: Arc::new(RpcClient::new_with_encoding(encoding, interface.into(), options)?),
            notifier,
            notification_mode,
            notification_channel,
            encoding, // notification_ctl: DuplexChannel::oneshot(),
        };

        // client.notifier.clone().map(|notifier|notifier.start());

        Ok(client)
    }

    /// Starts background tasks.
    pub async fn start(&self) -> Result<()> {
        match &self.notification_mode {
            NotificationMode::NotSynced => {
                self.notifier.clone().unwrap().start();
            }
            NotificationMode::Direct => {}
        }
        Ok(())
    }

    /// Stops background tasks.
    pub async fn stop(&self) -> Result<()> {
        match &self.notification_mode {
            NotificationMode::NotSynced => {
                log_info!("stop notifier...");
                self.notifier.as_ref().unwrap().stop().await?;
            }
            NotificationMode::Direct => {
                // log_info!("stop direct...");
                // self.notification_ctl.signal(()).await?;
            }
        }
        Ok(())
    }

    /// Starts a background async connection task connecting
    /// to the wRPC server.  If the supplied `block` call is `true`
    /// this function will block until the first successful
    /// connection.
    pub async fn connect(&self, block: bool) -> Result<Option<Listener>> {
        Ok(self.rpc.connect(block).await?)
    }

    pub async fn shutdown(&self) -> Result<()> {
        Ok(self.rpc.shutdown().await?)
    }

    /// A helper function that is not `async`, allowing connection
    /// process to be initiated from non-async contexts.
    pub fn connect_as_task(&self) -> Result<()> {
        let self_ = self.clone();
        workflow_core::task::spawn(async move {
            self_.rpc.connect(false).await.ok();
        });
        Ok(())
    }

    pub fn notification_channel_receiver(&self) -> Receiver<Notification> {
        self.notification_channel.receiver.clone()
    }

    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    // pub fn notification_relay_task(self: &Arc<Self>) -> Result<()> {
    //     let self_ = self.clone();
    //     spawn(async move {
    //         loop {
    //             select! {
    //                 _ = self_.notification_ctl.request.receiver.recv().fuse() => {
    //                     break
    //                 },
    //             }
    //         }
    //     });
    //     Ok(())
    // }
}

#[async_trait]
impl RpcApi for KaspaRpcClient {
    //
    // The following proc-macro iterates over the array of enum variants
    // generating a function for each variant as follows:
    //
    // async fn ping_call(&self, request : PingRequest) -> RpcResult<PingResponse> {
    //     let response: ClientResult<PingResponse> = self.rpc.call(RpcApiOps::Ping, request).await;
    //     Ok(response.map_err(|e| e.to_string())?)
    // }

    build_wrpc_client_interface!(
        RpcApiOps,
        [
            AddPeer,
            Ban,
            EstimateNetworkHashesPerSecond,
            GetBalanceByAddress,
            GetBalancesByAddresses,
            GetBlock,
            GetBlockCount,
            GetBlockDagInfo,
            GetBlocks,
            GetBlockTemplate,
            GetCoinSupply,
            GetConnectedPeerInfo,
            GetCurrentNetwork,
            GetHeaders,
            GetInfo,
            GetMempoolEntries,
            GetMempoolEntriesByAddresses,
            GetMempoolEntry,
            GetPeerAddresses,
            GetProcessMetrics,
            GetSelectedTipHash,
            GetSubnetwork,
            GetUtxosByAddresses,
            GetVirtualSelectedParentBlueScore,
            GetVirtualSelectedParentChainFromBlock,
            Ping,
            ResolveFinalityConflict,
            Shutdown,
            SubmitBlock,
            SubmitTransaction,
            Unban,
        ]
    );

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id and a channel receiver.
    fn register_new_listener(&self, sender: NotificationSender) -> ListenerId {
        match self.notification_mode {
            NotificationMode::NotSynced => self.notifier.as_ref().unwrap().register_new_listener(sender),
            NotificationMode::Direct => ListenerId::default(),
        }
    }

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener and drop its channel.
    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()> {
        match self.notification_mode {
            NotificationMode::NotSynced => {
                self.notifier.as_ref().unwrap().unregister_listener(id)?;
            }
            NotificationMode::Direct => {}
        }
        Ok(())
    }

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, id: ListenerId, notification_type: NotificationType) -> RpcResult<()> {
        match self.notification_mode {
            NotificationMode::NotSynced => {
                self.notifier.clone().unwrap().start_notify(id, notification_type.clone())?;
                let _response: SubscribeResponse =
                    self.rpc.call(RpcApiOps::Subscribe, notification_type).await.map_err(|err| err.to_string())?;
            }
            NotificationMode::Direct => {
                let _response: SubscribeResponse =
                    self.rpc.call(RpcApiOps::Subscribe, notification_type).await.map_err(|err| err.to_string())?;
            }
        }
        Ok(())
    }

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        match self.notification_mode {
            NotificationMode::NotSynced => {
                self.notifier.clone().unwrap().stop_notify(id, notification_type.clone())?;
                let _response: SubscribeResponse =
                    self.rpc.call(RpcApiOps::Unsubscribe, notification_type).await.map_err(|err| err.to_string())?;
            }
            NotificationMode::Direct => {
                let _response: SubscribeResponse =
                    self.rpc.call(RpcApiOps::Unsubscribe, notification_type).await.map_err(|err| err.to_string())?;
            }
        }
        Ok(())
    }
}
