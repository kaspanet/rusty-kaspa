#![allow(non_snake_case)]

use crate::imports::*;
use crate::Resolver;
use crate::{RpcEventCallback, RpcEventType, RpcEventTypeOrCallback};
use js_sys::{Function, Object};
use kaspa_addresses::{Address, AddressOrStringArrayT};
use kaspa_consensus_client::UtxoEntryReference;
use kaspa_consensus_core::network::{NetworkType, NetworkTypeT};
use kaspa_notify::connection::ChannelType;
use kaspa_notify::events::EventType;
use kaspa_notify::listener;
use kaspa_notify::notification::Notification as NotificationT;
use kaspa_rpc_core::api::ctl;
pub use kaspa_rpc_core::wasm::message::*;
pub use kaspa_rpc_macros::{
    build_wrpc_wasm_bindgen_interface, build_wrpc_wasm_bindgen_subscriptions, declare_typescript_wasm_interface as declare,
};
use kaspa_wasm_core::events::{get_event_targets, Sink};
pub use serde_wasm_bindgen::from_value;
use workflow_rpc::client::Ctl;
pub use workflow_rpc::client::IConnectOptions;
pub use workflow_rpc::encoding::Encoding as WrpcEncoding;
use workflow_wasm::callback;
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
         * Network identifier: `mainnet`, `testnet-10` etc.
         * `networkId` is required when using a resolver.
         */
        networkId?: NetworkId | string;
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
        let encoding = config.try_get::<Encoding>("encoding")?;
        let network_id = config.try_get::<NetworkId>("networkId")?;

        if resolver.is_some() && network_id.is_none() {
            return Err(Error::custom("networkId is required when using a resolver"));
        }

        Ok(RpcConfig { resolver, url, encoding, network_id })
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

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum NotificationEvent {
    All,
    Notification(EventType),
    RpcCtl(Ctl),
}

impl FromStr for NotificationEvent {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        if s == "*" {
            Ok(NotificationEvent::All)
        } else if let Ok(ctl) = Ctl::from_str(s) {
            Ok(NotificationEvent::RpcCtl(ctl))
        } else if let Ok(event) = EventType::from_str(s) {
            Ok(NotificationEvent::Notification(event))
        } else {
            Err(Error::custom(format!("Invalid notification event type: `{}`", s)))
        }
    }
}

impl TryFrom<JsValue> for NotificationEvent {
    type Error = Error;
    fn try_from(event: JsValue) -> Result<Self> {
        if let Some(event) = event.as_string() {
            event.parse()
        } else {
            Err(Error::custom(format!("Invalid notification event: `{:?}`", event)))
        }
    }
}

pub struct Inner {
    client: Arc<KaspaRpcClient>,
    resolver: Option<Resolver>,
    notification_task: AtomicBool,
    notification_ctl: DuplexChannel,
    callbacks: Arc<Mutex<AHashMap<NotificationEvent, Vec<Sink>>>>,
    listener_id: Arc<Mutex<Option<ListenerId>>>,
    notification_channel: Channel<kaspa_rpc_core::Notification>,
}

impl Inner {
    fn notification_callbacks(&self, event: NotificationEvent) -> Option<Vec<Sink>> {
        let notification_callbacks = self.callbacks.lock().unwrap();
        let all = notification_callbacks.get(&NotificationEvent::All).cloned();
        let target = notification_callbacks.get(&event).cloned();
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
///
/// Kaspa RPC client uses ([wRPC](https://github.com/workflow-rs/workflow-rs/tree/master/rpc))
/// interface to connect directly with Kaspa Node. wRPC supports
/// two types of encodings: `borsh` (binary, default) and `json`.
///
/// There are two ways to connect: Directly to any Kaspa Node or to a
/// community-maintained public node infrastructure using the {@link Resolver} class.
///
/// **Connecting to a public node using a resolver**
///
/// ```javascript
/// let rpc = new RpcClient({
///    resolver : new Resolver(),
///    networkId : "mainnet",
/// });
///
/// await rpc.connect();
/// ```
///
/// **Connecting to a Kaspa Node directly**
///
/// ```javascript
/// let rpc = new RpcClient({
///    // if port is not provided it will default
///    // to the default port for the networkId
///    url : "127.0.0.1",
///    networkId : "mainnet",
/// });
/// ```
///
/// **Example usage**
///
/// ```javascript
///
/// // Create a new RPC client with a URL
/// let rpc = new RpcClient({ url : "wss://<node-wrpc-address>" });
///
/// // Create a new RPC client with a resolver
/// // (networkId is required when using a resolver)
/// let rpc = new RpcClient({
///     resolver : new Resolver(),
///     networkId : "mainnet",
/// });
///
/// rpc.addEventListener("connect", async (event) => {
///     console.log("Connected to", rpc.url);
///     await rpc.subscribeDaaScore();
/// });
///
/// rpc.addEventListener("disconnect", (event) => {
///     console.log("Disconnected from", rpc.url);
/// });
///
/// try {
///     await rpc.connect();
/// } catch(err) {
///     console.log("Error connecting:", err);
/// }
///
/// ```
///
/// You can register event listeners to receive notifications from the RPC client
/// using {@link RpcClient.addEventListener} and {@link RpcClient.removeEventListener} functions.
///
/// **IMPORTANT:** If RPC is disconnected, upon reconnection you do not need
/// to re-register event listeners, but your have to re-subscribe for Kaspa node
/// notifications:
///
/// ```typescript
/// rpc.addEventListener("connect", async (event) => {
///     console.log("Connected to", rpc.url);
///     // re-subscribe each time we connect
///     await rpc.subscribeDaaScore();
///     // ... perform wallet address subscriptions
/// });
///
/// ```
///
/// If using NodeJS, it is important that {@link RpcClient.disconnect} is called before
/// the process exits to ensure that the WebSocket connection is properly closed.
/// Failure to do this will prevent the process from exiting.
///
/// @category Node RPC
///
#[wasm_bindgen(inspectable)]
#[derive(Clone, CastFromJs)]
pub struct RpcClient {
    // #[wasm_bindgen(skip)]
    pub(crate) inner: Arc<Inner>,
}

cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        #[wasm_bindgen(typescript_custom_section)]
        const TS_NOTIFY: &'static str = r#"
        interface RpcClient {
            /**
            * @param {RpcEventCallback} callback
            */
            addEventListener(callback:RpcEventCallback): void;
            /**
            * @param {RpcEventType} event
            * @param {RpcEventCallback} [callback]
            */
            addEventListener<M extends keyof RpcEventMap>(
                event: M,
                callback: (eventData: RpcEventMap[M]) => void
            )
        }"#;
    }
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
            KaspaRpcClient::new(encoding, url.as_deref(), resolver.clone().map(Into::into), network_id, None)
                .unwrap_or_else(|err| panic!("{err}")),
        );

        let rpc_client = RpcClient {
            inner: Arc::new(Inner {
                client,
                resolver,
                notification_task: AtomicBool::new(false),
                notification_ctl: DuplexChannel::oneshot(),
                callbacks: Arc::new(Default::default()),
                listener_id: Arc::new(Mutex::new(None)),
                notification_channel: Channel::unbounded(),
            }),
        };

        Ok(rpc_client)
    }
}

#[wasm_bindgen]
impl RpcClient {
    ///
    /// Create a new RPC client with optional {@link Encoding} and a `url`.
    ///
    /// @see {@link IRpcConfig} interface for more details.
    ///
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
        self.inner.resolver.clone()
    }

    /// Set the resolver for the RPC client.
    /// This setting will take effect on the next connection.
    #[wasm_bindgen(js_name = setResolver)]
    pub fn set_resolver(&self, resolver: Resolver) -> Result<()> {
        self.inner.client.set_resolver(resolver.into())?;
        Ok(())
    }

    /// Set the network id for the RPC client.
    /// This setting will take effect on the next connection.
    #[wasm_bindgen(js_name = setNetworkId)]
    pub fn set_network_id(&self, network_id: &NetworkId) -> Result<()> {
        self.inner.client.set_network_id(network_id)?;
        Ok(())
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
        let options = args.map(ConnectOptions::try_from).transpose()?;

        self.start_notification_task()?;
        self.inner.client.connect(options).await?;

        Ok(())
    }

    /// Disconnect from the Kaspa RPC server.
    pub async fn disconnect(&self) -> Result<()> {
        // disconnect the client first to receive the 'close' event
        self.inner.client.disconnect().await?;
        self.stop_notification_task().await?;
        Ok(())
    }

    /// Start background RPC services (automatically started when invoking {@link RpcClient.connect}).
    pub async fn start(&self) -> Result<()> {
        self.start_notification_task()?;
        self.inner.client.start().await?;
        Ok(())
    }

    /// Stop background RPC services (automatically stopped when invoking {@link RpcClient.disconnect}).
    pub async fn stop(&self) -> Result<()> {
        self.inner.client.stop().await?;
        self.stop_notification_task().await?;
        Ok(())
    }

    /// Triggers a disconnection on the underlying WebSocket
    /// if the WebSocket is in connected state.
    /// This is intended for debug purposes only.
    /// Can be used to test application reconnection logic.
    #[wasm_bindgen(js_name = "triggerAbort")]
    pub fn trigger_abort(&self) {
        self.inner.client.trigger_abort().ok();
    }

    ///
    /// Register an event listener callback.
    ///
    /// Registers a callback function to be executed when a specific event occurs.
    /// The callback function will receive an {@link RpcEvent} object with the event `type` and `data`.
    ///
    /// **RPC Subscriptions vs Event Listeners**
    ///
    /// Subscriptions are used to receive notifications from the RPC client.
    /// Event listeners are client-side application registrations that are
    /// triggered when notifications are received.
    ///
    /// If node is disconnected, upon reconnection you do not need to re-register event listeners,
    /// however, you have to re-subscribe for Kaspa node notifications. As such, it is recommended
    /// to register event listeners when the RPC `open` event is received.
    ///
    /// ```javascript
    /// rpc.addEventListener("connect", async (event) => {
    ///     console.log("Connected to", rpc.url);
    ///     await rpc.subscribeDaaScore();
    ///     // ... perform wallet address subscriptions
    /// });
    /// ```
    ///
    /// **Multiple events and listeners**
    ///
    /// `addEventListener` can be used to register multiple event listeners for the same event
    /// as well as the same event listener for multiple events.
    ///
    /// ```javascript
    /// // Registering a single event listener for multiple events:
    /// rpc.addEventListener(["connect", "disconnect"], (event) => {
    ///     console.log(event);
    /// });
    ///
    /// // Registering event listener for all events:
    /// // (by omitting the event type)
    /// rpc.addEventListener((event) => {
    ///     console.log(event);
    /// });
    ///
    /// // Registering multiple event listeners for the same event:
    /// rpc.addEventListener("connect", (event) => { // first listener
    ///     console.log(event);
    /// });
    /// rpc.addEventListener("connect", (event) => { // second listener
    ///     console.log(event);
    /// });
    /// ```
    ///
    /// **Use of context objects**
    ///
    /// You can also register an event with a `context` object. When the event is triggered,
    /// the `handleEvent` method of the `context` object will be called while `this` value
    /// will be set to the `context` object.
    /// ```javascript
    /// // Registering events with a context object:
    ///
    /// const context = {
    ///     someProperty: "someValue",
    ///     handleEvent: (event) => {
    ///         // the following will log "someValue"
    ///         console.log(this.someProperty);
    ///         console.log(event);
    ///     }
    /// };
    /// rpc.addEventListener(["connect","disconnect"], context);
    ///
    /// ```
    ///
    /// **General use examples**
    ///
    /// In TypeScript you can use {@link RpcEventType} enum (such as `RpcEventType.Connect`)
    /// or `string` (such as "connect") to register event listeners.
    /// In JavaScript you can only use `string`.
    ///
    /// ```typescript
    /// // Example usage (TypeScript):
    ///
    /// rpc.addEventListener(RpcEventType.Connect, (event) => {
    ///     console.log("Connected to", rpc.url);
    /// });
    ///
    /// rpc.addEventListener(RpcEventType.VirtualDaaScoreChanged, (event) => {
    ///     console.log(event.type,event.data);
    /// });
    /// await rpc.subscribeDaaScore();
    ///
    /// rpc.addEventListener(RpcEventType.BlockAdded, (event) => {
    ///     console.log(event.type,event.data);
    /// });
    /// await rpc.subscribeBlockAdded();
    ///
    /// // Example usage (JavaScript):
    ///
    /// rpc.addEventListener("virtual-daa-score-changed", (event) => {
    ///     console.log(event.type,event.data);
    /// });
    ///
    /// await rpc.subscribeDaaScore();
    /// rpc.addEventListener("block-added", (event) => {
    ///     console.log(event.type,event.data);
    /// });
    /// await rpc.subscribeBlockAdded();
    /// ```
    ///
    /// @see {@link RpcEventType} for a list of supported events.
    /// @see {@link RpcEventData} for the event data interface specification.
    /// @see {@link RpcClient.removeEventListener}, {@link RpcClient.removeAllEventListeners}
    ///
    #[wasm_bindgen(js_name = "addEventListener", skip_typescript)]
    pub fn add_event_listener(&self, event: RpcEventTypeOrCallback, callback: Option<RpcEventCallback>) -> Result<()> {
        if let Ok(sink) = Sink::try_from(&event) {
            let event = NotificationEvent::All;
            self.inner.callbacks.lock().unwrap().entry(event).or_default().push(sink);
            Ok(())
        } else if let Some(Ok(sink)) = callback.map(Sink::try_from) {
            let targets: Vec<NotificationEvent> = get_event_targets(event)?;
            for event in targets {
                self.inner.callbacks.lock().unwrap().entry(event).or_default().push(sink.clone());
            }
            Ok(())
        } else {
            Err(Error::custom("Invalid event listener callback"))
        }
    }

    ///
    /// Unregister an event listener.
    /// This function will remove the callback for the specified event.
    /// If the `callback` is not supplied, all callbacks will be
    /// removed for the specified event.
    ///
    /// @see {@link RpcClient.addEventListener}
    #[wasm_bindgen(js_name = "removeEventListener")]
    pub fn remove_event_listener(&self, event: RpcEventType, callback: Option<RpcEventCallback>) -> Result<()> {
        let mut callbacks = self.inner.callbacks.lock().unwrap();
        if let Ok(sink) = Sink::try_from(&event) {
            // remove callback from all events
            for (_, handlers) in callbacks.iter_mut() {
                handlers.retain(|handler| handler != &sink);
            }
        } else if let Some(Ok(sink)) = callback.map(Sink::try_from) {
            // remove callback from specific events
            let targets: Vec<NotificationEvent> = get_event_targets(event)?;
            for target in targets.into_iter() {
                callbacks.entry(target).and_modify(|handlers| {
                    handlers.retain(|handler| handler != &sink);
                });
            }
        } else {
            // remove all callbacks for the event
            let targets: Vec<NotificationEvent> = get_event_targets(event)?;
            for event in targets {
                callbacks.remove(&event);
            }
        }
        Ok(())
    }

    ///
    /// Unregister a single event listener callback from all events.
    ///
    ///
    ///
    #[wasm_bindgen(js_name = "clearEventListener")]
    pub fn clear_event_listener(&self, callback: RpcEventCallback) -> Result<()> {
        let sink = Sink::new(callback);
        let mut notification_callbacks = self.inner.callbacks.lock().unwrap();
        for (_, handlers) in notification_callbacks.iter_mut() {
            handlers.retain(|handler| handler != &sink);
        }
        Ok(())
    }

    ///
    /// Unregister all notification callbacks for all events.
    ///
    #[wasm_bindgen(js_name = "removeAllEventListeners")]
    pub fn remove_all_event_listeners(&self) -> Result<()> {
        *self.inner.callbacks.lock().unwrap() = Default::default();
        Ok(())
    }
}

impl RpcClient {
    pub fn new_with_rpc_client(client: Arc<KaspaRpcClient>) -> RpcClient {
        let resolver = client.resolver().map(Into::into);
        RpcClient {
            inner: Arc::new(Inner {
                client,
                resolver,
                notification_task: AtomicBool::new(false),
                notification_ctl: DuplexChannel::oneshot(),
                callbacks: Arc::new(Mutex::new(Default::default())),
                listener_id: Arc::new(Mutex::new(None)),
                notification_channel: Channel::unbounded(),
            }),
        }
    }

    pub fn listener_id(&self) -> Option<ListenerId> {
        *self.inner.listener_id.lock().unwrap()
    }

    pub fn client(&self) -> &Arc<KaspaRpcClient> {
        &self.inner.client
    }

    async fn stop_notification_task(&self) -> Result<()> {
        if self.inner.notification_task.load(Ordering::SeqCst) {
            self.inner.notification_ctl.signal(()).await.map_err(|err| JsError::new(&err.to_string()))?;
            self.inner.notification_task.store(false, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Notification task receives notifications and executes them on the
    /// user-supplied callback function.
    fn start_notification_task(&self) -> Result<()> {
        if self.inner.notification_task.load(Ordering::SeqCst) {
            return Ok(());
        }

        self.inner.notification_task.store(true, Ordering::SeqCst);

        let ctl_receiver = self.inner.notification_ctl.request.receiver.clone();
        let ctl_sender = self.inner.notification_ctl.response.sender.clone();
        let notification_receiver = self.inner.notification_channel.receiver.clone();
        let ctl_multiplexer_channel =
            self.inner.client.rpc_client().ctl_multiplexer().as_ref().expect("WASM32 RpcClient ctl_multiplexer is None").channel();
        let this = self.clone();

        spawn(async move {
            loop {
                select_biased! {
                    msg = ctl_multiplexer_channel.recv().fuse() => {
                        if let Ok(ctl) = msg {

                            match ctl {
                                Ctl::Connect => {
                                    let listener_id = this.inner.client.register_new_listener(ChannelConnection::new(
                                        "kaspa-wrpc-client-wasm",
                                        this.inner.notification_channel.sender.clone(),
                                        ChannelType::Persistent,
                                    ));
                                    *this.inner.listener_id.lock().unwrap() = Some(listener_id);
                                }
                                Ctl::Disconnect => {
                                    let listener_id = this.inner.listener_id.lock().unwrap().take();
                                    if let Some(listener_id) = listener_id {
                                        if let Err(err) = this.inner.client.unregister_listener(listener_id).await {
                                            log_error!("Error in unregister_listener: {:?}",err);
                                        }
                                    }
                                }
                            }

                            let event = NotificationEvent::RpcCtl(ctl);
                            if let Some(handlers) = this.inner.notification_callbacks(event) {
                                for handler in handlers.into_iter() {
                                    let event = Object::new();
                                    event.set("type", &ctl.to_string().into()).ok();
                                    event.set("rpc", &this.clone().into()).ok();
                                    if let Err(err) = handler.call(&event.into()) {
                                        log_error!("Error while executing RPC notification callback: {:?}",err);
                                    }
                                }
                            }
                        }
                    },
                    msg = notification_receiver.recv().fuse() => {
                        if let Ok(notification) = &msg {
                            match &notification {
                                kaspa_rpc_core::Notification::UtxosChanged(utxos_changed_notification) => {

                                    let event_type = EventType::UtxosChanged;
                                    let notification_event = NotificationEvent::Notification(event_type);
                                    if let Some(handlers) = this.inner.notification_callbacks(notification_event) {

                                        let UtxosChangedNotification { added, removed } = utxos_changed_notification;
                                        let added = js_sys::Array::from_iter(added.iter().map(UtxoEntryReference::from).map(JsValue::from));
                                        let removed = js_sys::Array::from_iter(removed.iter().map(UtxoEntryReference::from).map(JsValue::from));
                                        let notification = Object::new();
                                        notification.set("added", &added).unwrap();
                                        notification.set("removed", &removed).unwrap();

                                        for handler in handlers.into_iter() {
                                            let event = Object::new();
                                            let event_type_value = to_value(&event_type).unwrap();
                                            event.set("type", &event_type_value).expect("setting event type");
                                            event.set("data", &notification).expect("setting event data");
                                            if let Err(err) = handler.call(&event.into()) {
                                                log_error!("Error while executing RPC notification callback: {:?}",err);
                                            }
                                        }
                                    }
                                },
                                _ => {
                                    let event_type = notification.event_type();
                                    let notification_event = NotificationEvent::Notification(event_type);
                                    if let Some(handlers) = this.inner.notification_callbacks(notification_event) {
                                        for handler in handlers.into_iter() {
                                            let event = Object::new();
                                            let event_type_value = to_value(&event_type).unwrap();
                                            event.set("type", &event_type_value).expect("setting event type");
                                            event.set("data", &notification.to_value().unwrap()).expect("setting event data");
                                            if let Err(err) = handler.call(&event.into()) {
                                                log_error!("Error while executing RPC notification callback: {:?}",err);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ = ctl_receiver.recv().fuse() => {
                        break;
                    },

                }
            }

            if let Some(listener_id) = this.listener_id() {
                this.inner.listener_id.lock().unwrap().take();
                if let Err(err) = this.inner.client.unregister_listener(listener_id).await {
                    log_error!("Error in unregister_listener: {:?}", err);
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
    pub fn default_port(encoding: WrpcEncoding, network: &NetworkTypeT) -> Result<u16> {
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
    /// Manage subscription for a virtual DAA score changed notification event.
    /// Virtual DAA score changed notification event is produced when the virtual
    /// Difficulty Adjustment Algorithm (DAA) score changes in the Kaspa BlockDAG.
    #[wasm_bindgen(js_name = subscribeVirtualDaaScoreChanged)]
    pub async fn subscribe_daa_score(&self) -> Result<()> {
        if let Some(listener_id) = self.listener_id() {
            self.inner.client.stop_notify(listener_id, Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        } else {
            log_error!("RPC unsubscribe on a closed connection");
        }
        Ok(())
    }

    /// Manage subscription for a virtual DAA score changed notification event.
    /// Virtual DAA score changed notification event is produced when the virtual
    /// Difficulty Adjustment Algorithm (DAA) score changes in the Kaspa BlockDAG.
    #[wasm_bindgen(js_name = unsubscribeVirtualDaaScoreChanged)]
    pub async fn unsubscribe_daa_score(&self) -> Result<()> {
        if let Some(listener_id) = self.listener_id() {
            self.inner.client.stop_notify(listener_id, Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        } else {
            log_error!("RPC unsubscribe on a closed connection");
        }
        Ok(())
    }

    /// Subscribe for a UTXOs changed notification event.
    /// UTXOs changed notification event is produced when the set
    /// of unspent transaction outputs (UTXOs) changes in the
    /// Kaspa BlockDAG. The event notification will be scoped to the
    /// provided list of addresses.
    #[wasm_bindgen(js_name = subscribeUtxosChanged)]
    pub async fn subscribe_utxos_changed(&self, addresses: AddressOrStringArrayT) -> Result<()> {
        if let Some(listener_id) = self.listener_id() {
            let addresses: Vec<Address> = addresses.try_into()?;
            self.inner.client.start_notify(listener_id, Scope::UtxosChanged(UtxosChangedScope { addresses })).await?;
        } else {
            log_error!("RPC subscribe on a closed connection");
        }

        Ok(())
    }

    /// Unsubscribe from UTXOs changed notification event
    /// for a specific set of addresses.
    #[wasm_bindgen(js_name = unsubscribeUtxosChanged)]
    pub async fn unsubscribe_utxos_changed(&self, addresses: AddressOrStringArrayT) -> Result<()> {
        if let Some(listener_id) = self.listener_id() {
            let addresses: Vec<Address> = addresses.try_into()?;
            self.inner.client.stop_notify(listener_id, Scope::UtxosChanged(UtxosChangedScope { addresses })).await?;
        } else {
            log_error!("RPC unsubscribe on a closed connection");
        }
        Ok(())
    }

    // TODO: scope variant with field functions

    /// Manage subscription for a virtual chain changed notification event.
    /// Virtual chain changed notification event is produced when the virtual
    /// chain changes in the Kaspa BlockDAG.
    #[wasm_bindgen(js_name = subscribeVirtualChainChanged)]
    pub async fn subscribe_virtual_chain_changed(&self, include_accepted_transaction_ids: bool) -> Result<()> {
        if let Some(listener_id) = self.listener_id() {
            self.inner
                .client
                .start_notify(listener_id, Scope::VirtualChainChanged(VirtualChainChangedScope { include_accepted_transaction_ids }))
                .await?;
        } else {
            log_error!("RPC subscribe on a closed connection");
        }
        Ok(())
    }

    /// Manage subscription for a virtual chain changed notification event.
    /// Virtual chain changed notification event is produced when the virtual
    /// chain changes in the Kaspa BlockDAG.
    #[wasm_bindgen(js_name = unsubscribeVirtualChainChanged)]
    pub async fn unsubscribe_virtual_chain_changed(&self, include_accepted_transaction_ids: bool) -> Result<()> {
        if let Some(listener_id) = self.listener_id() {
            self.inner
                .client
                .stop_notify(listener_id, Scope::VirtualChainChanged(VirtualChainChangedScope { include_accepted_transaction_ids }))
                .await?;
        } else {
            log_error!("RPC unsubscribe on a closed connection");
        }
        Ok(())
    }
}

// Build subscribe functions
build_wrpc_wasm_bindgen_subscriptions!([
    // Manually implemented subscriptions (above)
    // - VirtualChainChanged, // can't used this here due to non-C-style enum variant
    // - UtxosChanged, // can't used this here due to non-C-style enum variant
    // - VirtualDaaScoreChanged,
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
