use crate::error::Error;
use crate::imports::*;
use crate::result::Result;
use crate::utxo as native;
use kaspa_wrpc_client::wasm::RpcClient;
use workflow_wasm::channel::EventDispatcher;

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
    pub async fn ctor(js_value: JsValue) -> Result<UtxoProcessor> {
        let UtxoProcessorCreateArgs { rpc, network_id } = js_value.try_into()?;
        let rpc_api: Arc<DynRpcApi> = rpc.client().clone();
        let rpc_ctl = rpc.client().rpc_ctl().clone();
        let rpc_binding = Rpc::new(rpc_api, rpc_ctl);
        let inner = native::UtxoProcessor::new(Some(rpc_binding), Some(network_id), None);
        let events = EventDispatcher::new();

        inner.start().await?;

        Ok(UtxoProcessor { inner, rpc, events })
    }

    // TODO - discuss async ctor interface
    // pub async fn start(&self) -> Result<()> {
    //     self.inner().start().await
    // }

    pub async fn shutdown(&self) -> Result<()> {
        self.inner().stop().await
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

impl TryFrom<JsValue> for UtxoProcessorCreateArgs {
    type Error = Error;
    fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&value) {
            let rpc = object.get_value("rpc")?;
            let rpc = ref_from_abi!(RpcClient, &rpc)?;
            let network_id = object.get::<NetworkId>("networkId")?;
            Ok(UtxoProcessorCreateArgs { rpc, network_id })
        } else {
            Err(Error::custom("UtxoProcessor: suppliedd value must be an object"))
        }
    }
}
