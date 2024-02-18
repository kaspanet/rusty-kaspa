use crate::error::Error;
use crate::imports::*;
use crate::result::Result;
use crate::utxo as native;
use crate::wasm::dispatcher::EventDispatcher;
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use kaspa_wrpc_wasm::RpcClient;

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

///
/// UtxoProcessor class is the main coordinator that manages UTXO processing
/// between multiple UtxoContext instances. It acts as a bridge between the
/// Kaspa node RPC connection, address subscriptions and UtxoContext instances.
///
/// UtxoProcessor provides two properties:
///   - `rpc`: the {@link RpcClient} used for network communication
///   - `events`: the {@link EventDispatcher} used for {@link IWalletEvent} notifications
///
/// @see {@link IUtxoProcessorArgs},
/// {@link UtxoContext},
/// {@link RpcClient},
/// {@link NetworkId},
/// {@link EventDispatcher}
/// {@link IConnectEvent}
/// {@link IDisconnectEvent}
/// @category Wallet SDK
///
#[derive(Clone)]
#[wasm_bindgen(inspectable)]
pub struct UtxoProcessor {
    inner: native::UtxoProcessor,
    #[wasm_bindgen(getter_with_clone)]
    pub rpc: RpcClient,
    #[wasm_bindgen(getter_with_clone)]
    pub events: EventDispatcher,
}

impl UtxoProcessor {
    pub fn inner(&self) -> &native::UtxoProcessor {
        &self.inner
    }
}

#[wasm_bindgen]
impl UtxoProcessor {
    #[wasm_bindgen(constructor)]
    pub async fn ctor(js_value: IUtxoProcessorArgs) -> Result<UtxoProcessor> {
        let UtxoProcessorCreateArgs { rpc, network_id } = js_value.try_into()?;
        let rpc_api: Arc<DynRpcApi> = rpc.client().clone();
        let rpc_ctl = rpc.client().rpc_ctl().clone();
        let rpc_binding = Rpc::new(rpc_api, rpc_ctl);
        let inner = native::UtxoProcessor::new(Some(rpc_binding), Some(network_id), None, None);
        let events = EventDispatcher::new();

        inner.start().await?;
        events.start_notification_task(inner.multiplexer()).await?;

        Ok(UtxoProcessor { inner, rpc, events })
    }

    // TODO - discuss async ctor interface
    // pub async fn start(&self) -> Result<()> {
    //     self.inner().start().await
    // }

    pub async fn shutdown(&self) -> Result<()> {
        self.inner().stop().await?;
        self.events.stop_notification_task().await?;
        Ok(())
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
