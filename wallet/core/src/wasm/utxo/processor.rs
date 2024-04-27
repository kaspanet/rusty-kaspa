use crate::error::Error;
use crate::events::{EventKind, Events};
use crate::imports::*;
use crate::result::Result;
use crate::utxo as native;
use crate::wasm::notify::{UtxoProcessorEventTarget, UtxoProcessorNotificationCallback, UtxoProcessorNotificationTypeOrCallback};
use kaspa_consensus_core::network::NetworkIdT;
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use kaspa_wasm_core::events::{get_event_targets, Sink};
use kaspa_wrpc_wasm::RpcClient;
use workflow_log::log_error;

declare! {
    IUtxoProcessorArgs,
    r#"
    /**
     * UtxoProcessor constructor arguments.
     * 
     * @see {@link UtxoProcessor}, {@link UtxoContext}, {@link RpcClient}, {@link NetworkId}
     * @category Wallet SDK
     */
    export interface IUtxoProcessorArgs {
        /**
         * The RPC client to use for network communication.
         */
        rpc : RpcClient;
        networkId : NetworkId | string;
    }
    "#,
}

pub struct Inner {
    processor: native::UtxoProcessor,
    rpc: RpcClient,

    callbacks: Mutex<AHashMap<EventKind, Vec<Sink>>>,
    task_running: AtomicBool,
    task_ctl: DuplexChannel,
}

impl Inner {
    fn callbacks(&self, event: EventKind) -> Option<Vec<Sink>> {
        let callbacks = self.callbacks.lock().unwrap();
        let all = callbacks.get(&EventKind::All).cloned();
        let target = callbacks.get(&event).cloned();
        match (all, target) {
            (Some(mut vec_all), Some(vec_target)) => {
                vec_all.extend(vec_target);
                Some(vec_all)
            }
            (Some(vec_all), None) => Some(vec_all),
            (None, Some(vec_target)) => Some(vec_target),
            (None, None) => None,
        }
    }
}

cfg_if! {
    if #[cfg(any(feature = "wasm32-core", feature = "wasm32-sdk"))] {
        #[wasm_bindgen(typescript_custom_section)]
        const TS_NOTIFY: &'static str = r#"
        interface UtxoProcessor {
            /**
            * @param {UtxoProcessorNotificationCallback} callback
            */
            addEventListener(callback:UtxoProcessorNotificationCallback): void;
            /**
            * @param {UtxoProcessorEventType} event
            * @param {UtxoProcessorNotificationCallback} [callback]
            */
            addEventListener<M extends keyof UtxoProcessorEventMap>(
                event: M,
                callback: (eventData: UtxoProcessorEventMap[M]) => void
            )
        }"#;
    }
}

///
/// UtxoProcessor class is the main coordinator that manages UTXO processing
/// between multiple UtxoContext instances. It acts as a bridge between the
/// Kaspa node RPC connection, address subscriptions and UtxoContext instances.
///
/// @see {@link IUtxoProcessorArgs},
/// {@link UtxoContext},
/// {@link RpcClient},
/// {@link NetworkId},
/// {@link IConnectEvent}
/// {@link IDisconnectEvent}
/// @category Wallet SDK
///
#[derive(Clone, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct UtxoProcessor {
    inner: Arc<Inner>,
}

#[wasm_bindgen]
impl UtxoProcessor {
    /// UtxoProcessor constructor.
    ///
    ///
    ///
    /// @see {@link IUtxoProcessorArgs}
    #[wasm_bindgen(constructor)]
    pub fn ctor(js_value: IUtxoProcessorArgs) -> Result<UtxoProcessor> {
        let UtxoProcessorCreateArgs { rpc, network_id } = js_value.try_into()?;
        let rpc_api: Arc<DynRpcApi> = rpc.client().clone();
        let rpc_ctl = rpc.client().rpc_ctl().clone();
        let rpc_binding = Rpc::new(rpc_api, rpc_ctl);
        let processor = native::UtxoProcessor::new(Some(rpc_binding), Some(network_id), None, None);

        let this = UtxoProcessor {
            inner: Arc::new(Inner {
                processor: processor.clone(),
                rpc,
                callbacks: Mutex::new(AHashMap::new()),
                task_running: AtomicBool::new(false),
                task_ctl: DuplexChannel::oneshot(),
            }),
        };

        Ok(this)
    }

    /// Starts the UtxoProcessor and begins processing UTXO and other notifications.
    pub async fn start(&self) -> Result<()> {
        self.start_notification_task(self.inner.processor.multiplexer()).await?;
        self.inner.processor.start().await?;
        Ok(())
    }

    /// Stops the UtxoProcessor and ends processing UTXO and other notifications.
    pub async fn stop(&self) -> Result<()> {
        self.inner.processor.stop().await?;
        self.stop_notification_task().await?;
        Ok(())
    }

    #[wasm_bindgen(getter)]
    pub fn rpc(&self) -> RpcClient {
        self.inner.rpc.clone()
    }

    #[wasm_bindgen(getter, js_name = "networkId")]
    pub fn network_id(&self) -> Option<String> {
        self.inner.processor.network_id().ok().map(|network_id| network_id.to_string())
    }

    #[wasm_bindgen(js_name = "setNetworkId")]
    pub fn set_network_id(&self, network_id: &NetworkIdT) -> Result<()> {
        let network_id = NetworkId::try_cast_from(network_id)?;
        self.inner.processor.set_network_id(network_id.as_ref());
        Ok(())
    }
}

impl TryCastFromJs for UtxoProcessor {
    type Error = workflow_wasm::error::Error;
    fn try_cast_from(value: impl AsRef<JsValue>) -> Result<Cast<Self>, Self::Error> {
        Self::try_ref_from_js_value_as_cast(value)
    }
}

pub struct UtxoProcessorCreateArgs {
    rpc: RpcClient,
    network_id: NetworkId,
}

impl TryFrom<IUtxoProcessorArgs> for UtxoProcessorCreateArgs {
    type Error = Error;
    fn try_from(value: IUtxoProcessorArgs) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&value) {
            let rpc = object.get_value("rpc")?;
            let rpc = RpcClient::try_ref_from_js_value(&rpc)?.clone();
            let network_id = object.get::<NetworkId>("networkId")?;
            Ok(UtxoProcessorCreateArgs { rpc, network_id })
        } else {
            Err(Error::custom("UtxoProcessor: supplied value must be an object"))
        }
    }
}

impl UtxoProcessor {
    pub fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    pub fn processor(&self) -> &native::UtxoProcessor {
        &self.inner.processor
    }

    pub async fn start_notification_task(&self, multiplexer: &Multiplexer<Box<Events>>) -> Result<()> {
        let inner = self.inner.clone();

        if inner.task_running.load(Ordering::SeqCst) {
            log_error!("You are calling `UtxoProcessor.start()` twice without calling `UtxoProcessor.stop()`!");
            panic!("UtxoProcessor background task is already running");
        } else {
            inner.task_running.store(true, Ordering::SeqCst);
        }

        let ctl_receiver = inner.task_ctl.request.receiver.clone();
        let ctl_sender = inner.task_ctl.response.sender.clone();
        let channel = multiplexer.channel();

        spawn(async move {
            loop {
                select! {
                    _ = ctl_receiver.recv().fuse() => {
                        break;
                    },
                    msg = channel.receiver.recv().fuse() => {
                        if let Ok(notification) = &msg {
                            let event_type = EventKind::from(notification.as_ref());
                            let callbacks = inner.callbacks(event_type);
                            if let Some(handlers) = callbacks {
                                for handler in handlers.into_iter() {
                                    let value = notification.as_ref().to_js_value();
                                    if let Err(err) = handler.call(&value) {
                                        log_error!("Error while executing notification callback: {:?}", err);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            channel.close();
            inner.task_running.store(false, Ordering::SeqCst);
            ctl_sender.send(()).await.ok();
        });

        Ok(())
    }

    pub async fn stop_notification_task(&self) -> Result<()> {
        let inner = &self.inner;
        if inner.task_running.load(Ordering::SeqCst) {
            inner.task_ctl.signal(()).await.map_err(|err| JsValue::from_str(&err.to_string()))?;
        }
        Ok(())
    }
}

#[wasm_bindgen]
impl UtxoProcessor {
    #[wasm_bindgen(js_name = "addEventListener", skip_typescript)]
    pub fn add_event_listener(
        &self,
        event: UtxoProcessorNotificationTypeOrCallback,
        callback: Option<UtxoProcessorNotificationCallback>,
    ) -> Result<()> {
        if let Ok(sink) = Sink::try_from(&event) {
            let event = EventKind::All;
            self.inner.callbacks.lock().unwrap().entry(event).or_default().push(sink);
            Ok(())
        } else if let Some(Ok(sink)) = callback.map(Sink::try_from) {
            let targets: Vec<EventKind> = get_event_targets(event)?;
            for event in targets {
                self.inner.callbacks.lock().unwrap().entry(event).or_default().push(sink.clone());
            }
            Ok(())
        } else {
            Err(Error::custom("Invalid event listener callback"))
        }
    }

    #[wasm_bindgen(js_name = "removeEventListener")]
    pub fn remove_event_listener(
        &self,
        event: UtxoProcessorEventTarget,
        callback: Option<UtxoProcessorNotificationCallback>,
    ) -> Result<()> {
        let mut callbacks = self.inner.callbacks.lock().unwrap();
        if let Ok(sink) = Sink::try_from(&event) {
            // remove callback from all events
            for (_, handlers) in callbacks.iter_mut() {
                handlers.retain(|handler| handler != &sink);
            }
        } else if let Some(Ok(sink)) = callback.map(Sink::try_from) {
            // remove callback from specific events
            let targets: Vec<EventKind> = get_event_targets(event)?;
            for target in targets.into_iter() {
                callbacks.entry(target).and_modify(|handlers| {
                    handlers.retain(|handler| handler != &sink);
                });
            }
        } else {
            // remove all callbacks for the event
            let targets: Vec<EventKind> = get_event_targets(event)?;
            for event in targets {
                callbacks.remove(&event);
            }
        }
        Ok(())
    }
}
