use crate::error::Error;
use crate::imports::*;
use crate::result::Result;
use crate::utxo as native;
use kaspa_wrpc_client::wasm::RpcClient;
use workflow_wasm::channel::MultiplexerClient;

#[derive(Clone)]
#[wasm_bindgen(inspectable)]
pub struct UtxoProcessor {
    inner: native::UtxoProcessor,
    #[wasm_bindgen(getter_with_clone)]
    pub rpc: RpcClient,
    #[wasm_bindgen(getter_with_clone)]
    pub events: MultiplexerClient,
}

impl UtxoProcessor {
    pub fn inner(&self) -> &native::UtxoProcessor {
        &self.inner
    }
}

#[wasm_bindgen]
impl UtxoProcessor {
    pub fn ctor(js_value: JsValue) -> Result<UtxoProcessor> {
        let UtxoProcessorCreateArgs { rpc, network_id } = js_value.try_into()?;
        let rpc_client: Arc<DynRpcApi> = rpc.client().clone();
        let inner = native::UtxoProcessor::new(&rpc_client, Some(network_id), None);

        // - TODO
        let events = MultiplexerClient::new();
        Ok(UtxoProcessor { inner, rpc, events })
        // - TODO
    }
}

// impl TryFrom<JsValue> for UtxoProcessor {
//     type Error = Error;
//     fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
//         Ok(ref_from_abi!(UtxoProcessor, &value)?)
//     }
// }

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

            let network_id = object.get::<NetworkId>("network_id")?;

            // let network_id = NetworkId::try_from(object.get("network_id"))?;
            // let

            Ok(UtxoProcessorCreateArgs { rpc, network_id })
        } else {
            Err(Error::custom("UtxoProcessor: suppliedd value must be an object"))
        }
    }
}
