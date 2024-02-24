use crate::error::Error;
use crate::events::{EventKind, Events};
use crate::imports::*;
use crate::result::Result;
use crate::utxo as native;
use crate::wasm::events::Sink;
use crate::wasm::notify::{UtxoProcessorEventTarget, UtxoProcessorNotificationCallback, UtxoProcessorNotificationTypeOrCallback};
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use kaspa_wrpc_wasm::RpcClient;
use serde_wasm_bindgen::to_value;
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
#[derive(Clone)]
#[wasm_bindgen(inspectable)]
pub struct UtxoProcessor {
    inner: Arc<Inner>,
}

#[wasm_bindgen]
impl UtxoProcessor {
    #[wasm_bindgen(constructor)]
    pub async fn ctor(js_value: IUtxoProcessorArgs) -> Result<UtxoProcessor> {
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

        this.start_notification_task(processor.multiplexer()).await?;
        processor.start().await?;

        Ok(this)
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.inner.processor.stop().await?;
        self.stop_notification_task().await?;
        Ok(())
    }

    #[wasm_bindgen(getter)]
    pub fn rpc(&self) -> RpcClient {
        self.inner.rpc.clone()
    }
}

impl TryFrom<JsValue> for UtxoProcessor {
    type Error = Error;
    fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
        Ok(ref_from_abi!(UtxoProcessor, &value)?)
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
            let rpc = ref_from_abi!(RpcClient, &rpc)?;
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
            panic!("ReflectorClient task is already running");
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
                                    let value = to_value(&notification).unwrap();
                                    if let Err(err) = handler.0.call1(&JsValue::undefined(), &value) {
                                        log_error!("Error while executing RPC notification callback: {:?}", err);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            channel.close();
            ctl_sender.send(()).await.ok();
        });

        Ok(())
    }

    pub async fn stop_notification_task(&self) -> Result<()> {
        let inner = &self.inner;
        if inner.task_running.load(Ordering::SeqCst) {
            inner.task_running.store(false, Ordering::SeqCst);
            inner.task_ctl.signal(()).await.map_err(|err| JsValue::from_str(&err.to_string()))?;
        }
        Ok(())
    }
}

#[wasm_bindgen]
impl UtxoProcessor {
    #[wasm_bindgen(js_name = "addEventListener")]
    pub fn add_event_listener(
        &self,
        event: UtxoProcessorNotificationTypeOrCallback,
        callback: Option<UtxoProcessorNotificationCallback>,
    ) -> Result<()> {
        if event.is_function() {
            let callback = Function::from(event);
            let event = EventKind::All;
            self.inner.callbacks.lock().unwrap().entry(event).or_default().push(Sink(callback));
        } else if let Some(callback) = callback {
            let event = EventKind::try_from(JsValue::from(event))?;
            self.inner.callbacks.lock().unwrap().entry(event).or_default().push(Sink(callback.into()));
        } else {
            return Err(Error::custom("Invalid event listener callback"));
        }
        Ok(())
    }

    #[wasm_bindgen(js_name = "removeEventListener")]
    pub fn remove_event_listener(
        &self,
        event: UtxoProcessorEventTarget,
        callback: Option<UtxoProcessorNotificationCallback>,
    ) -> Result<()> {
        let event = EventKind::try_from(JsValue::from(event))?;

        if let Some(callback) = callback {
            let sink = Sink(callback.into());

            let mut notification_callbacks = self.inner.callbacks.lock().unwrap();
            match event {
                EventKind::All => {
                    if let Some(handlers) = notification_callbacks.get_mut(&EventKind::All) {
                        handlers.retain(|handler| handler != &sink);
                    }
                }
                _ => {
                    if let Some(handlers) = notification_callbacks.get_mut(&event) {
                        handlers.retain(|handler| handler != &sink);
                    }
                }
            }
        } else {
            self.inner.callbacks.lock().unwrap().remove(&event);
        }
        Ok(())
    }
}
