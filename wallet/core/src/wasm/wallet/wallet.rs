use crate::imports::*;
use crate::storage::local::interface::LocalStore;
use crate::storage::WalletDescriptor;
use crate::wallet as native;
use crate::wasm::notify::{WalletEventTarget, WalletNotificationCallback, WalletNotificationTypeOrCallback};
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use kaspa_wasm_core::events::{get_event_targets, Sink};
use kaspa_wrpc_wasm::{IConnectOptions, Resolver, RpcClient, RpcConfig, WrpcEncoding};

declare! {
    IWalletConfig,
    r#"
    /**
     * 
     * 
     * @category  Wallet API
     */
    export interface IWalletConfig {
        /**
         * `resident` is a boolean indicating if the wallet should not be stored on the permanent medium.
         */
        resident?: boolean;
        networkId?: NetworkId | string;
        encoding?: Encoding | string;
        url?: string;
        resolver?: Resolver;
    }
    "#,
}

#[derive(Default)]
struct WalletCtorArgs {
    resident: bool,
    network_id: Option<NetworkId>,
    encoding: Option<WrpcEncoding>,
    url: Option<String>,
    resolver: Option<Resolver>,
}

impl TryFrom<JsValue> for WalletCtorArgs {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self> {
        if let Some(object) = Object::try_from(&js_value) {
            let resident = object.get_value("resident")?.as_bool().unwrap_or(false);
            let network_id = object.try_get::<NetworkId>("networkId")?;
            let encoding = object.try_get::<WrpcEncoding>("encoding")?;
            let url = object.get_value("url")?.as_string();
            let resolver = object.try_get("resolver")?;

            Ok(Self { resident, network_id, encoding, url, resolver })
        } else {
            Ok(WalletCtorArgs::default())
        }
    }
}

struct Inner {
    wallet: Arc<native::Wallet>,
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
/// Wallet class is the main coordinator that manages integrated wallet operations.
///
/// The Wallet class encapsulates {@link UtxoProcessor} and provides internal
/// account management using {@link UtxoContext} instances. It acts as a bridge
/// between the integrated Wallet subsystem providing a high-level interface
/// for wallet key and account management.
///
/// The Rusty Kaspa is developed in Rust, and the Wallet class is a Rust implementation
/// exposed to the JavaScript/TypeScript environment using the WebAssembly (WASM32) interface.
/// As such, the Wallet implementation can be powered up using native Rust or built
/// as a WebAssembly module and used in the browser or Node.js environment.
///
/// When using Rust native or NodeJS environment, all wallet data is stored on the local
/// filesystem.  When using WASM32 build in the web browser, the wallet data is stored
/// in the browser's `localStorage` and transaction records are stored in the `IndexedDB`.
///
/// The Wallet API can create multiple wallet instances, however, only one wallet instance
/// can be active at a time.
///
/// The wallet implementation is designed to be efficient and support a large number
/// of accounts. Accounts reside in storage and can be loaded and activated as needed.
/// A `loaded` account contains all account information loaded from the permanent storage
/// whereas an `active` account monitors the UTXO set and provides notifications for
/// incoming and outgoing transactions as well as balance updates.
///
/// The Wallet API communicates with the client using resource identifiers. These include
/// account IDs, private key IDs, transaction IDs, etc. It is the responsibility of the
/// client to track these resource identifiers at runtime.
///
/// @see {@link IWalletConfig},
///
/// @category Wallet API
///
#[wasm_bindgen(inspectable)]
#[derive(Clone)]
pub struct Wallet {
    inner: Arc<Inner>,
}

cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        #[wasm_bindgen(typescript_custom_section)]
        const TS_NOTIFY: &'static str = r#"
        interface Wallet {
            /**
            * @param {WalletNotificationCallback} callback
            */
            addEventListener(callback:WalletNotificationCallback): void;
            /**
            * @param {WalletEventType} event
            * @param {WalletNotificationCallback} [callback]
            */
            addEventListener<M extends keyof WalletEventMap>(
                event: M,
                callback: (eventData: WalletEventMap[M]) => void
            )
        }"#;
    }
}

#[wasm_bindgen]
impl Wallet {
    #[wasm_bindgen(constructor)]
    pub fn constructor(config: IWalletConfig) -> Result<Wallet> {
        let WalletCtorArgs { resident, network_id, encoding, url, resolver } = WalletCtorArgs::try_from(JsValue::from(config))?;

        let store = Arc::new(LocalStore::try_new(resident)?);

        let rpc_config = RpcConfig { url, resolver, encoding, network_id };

        let rpc = RpcClient::new(Some(rpc_config))?;
        let rpc_api: Arc<DynRpcApi> = rpc.client().rpc_api().clone();
        let rpc_ctl = rpc.client().rpc_ctl().clone();
        let rpc_binding = Rpc::new(rpc_api, rpc_ctl);
        let wallet = Arc::new(native::Wallet::try_with_rpc(Some(rpc_binding), store, network_id)?);

        Ok(Self {
            inner: Arc::new(Inner {
                wallet,
                rpc,
                callbacks: Mutex::new(AHashMap::new()),
                task_running: AtomicBool::new(false),
                task_ctl: DuplexChannel::oneshot(),
            }),
        })
    }

    #[wasm_bindgen(getter, js_name = "rpc")]
    pub fn rpc(&self) -> RpcClient {
        self.inner.rpc.clone()
    }

    /// @remarks This is a local property indicating
    /// if the wallet is currently open.
    #[wasm_bindgen(getter, js_name = "isOpen")]
    pub fn is_open(&self) -> bool {
        self.wallet().is_open()
    }

    /// @remarks This is a local property indicating
    /// if the node is currently synced.
    #[wasm_bindgen(getter, js_name = "isSynced")]
    pub fn is_synced(&self) -> bool {
        self.wallet().is_synced()
    }

    #[wasm_bindgen(getter, js_name = "descriptor")]
    pub fn descriptor(&self) -> Option<WalletDescriptor> {
        self.wallet().descriptor()
    }

    /// Check if a wallet with a given name exists.
    pub async fn exists(&self, name: Option<String>) -> Result<bool> {
        self.wallet().exists(name.as_deref()).await
    }

    pub async fn start(&self) -> Result<()> {
        self.start_notification_task(self.wallet().multiplexer()).await?;
        self.wallet().start().await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.wallet().stop().await?;
        self.stop_notification_task().await?;
        Ok(())
    }

    pub async fn connect(&self, args: Option<IConnectOptions>) -> Result<()> {
        self.inner.rpc.connect(args).await?;
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.inner.rpc.client().disconnect().await?;
        Ok(())
    }

    #[wasm_bindgen(js_name = "addEventListener", skip_typescript)]
    pub fn add_event_listener(
        &self,
        event: WalletNotificationTypeOrCallback,
        callback: Option<WalletNotificationCallback>,
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
    pub fn remove_event_listener(&self, event: WalletEventTarget, callback: Option<WalletNotificationCallback>) -> Result<()> {
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

impl Wallet {
    pub fn wallet(&self) -> &Arc<native::Wallet> {
        &self.inner.wallet
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
