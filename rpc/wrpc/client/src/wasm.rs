use super::client::*;
use kaspa_rpc_macros::build_wrpc_wasm_bindgen_interface;
use rpc_core::{api::rpc::RpcApi, error::RpcResult, prelude::*};
use serde_wasm_bindgen::*;
use wasm_bindgen::prelude::*;
use workflow_log::log_info;
use workflow_rpc::client::prelude::Encoding;
type JsResult<T> = std::result::Result<T, JsError>;

#[wasm_bindgen]
pub struct RpcClient {
    client: KaspaRpcClient,
}

#[wasm_bindgen]
impl RpcClient {
    #[wasm_bindgen(constructor)]
    pub fn new(encoding: Encoding, url: &str) -> RpcClient {
        RpcClient { client: KaspaRpcClient::new(encoding, url).unwrap_or_else(|err| panic!("{err}")) }
    }

    pub async fn connect(&self) -> JsResult<()> {
        self.client.start().await?;
        self.client.connect(true).await?; //.unwrap();
        Ok(())
    }

    pub async fn disconnect(&self) -> JsResult<()> {
        self.client.stop().await?;
        self.client.shutdown().await?;
        Ok(())
    }
}

// #[wasm_bindgen]
// impl RpcClient {
//     // pub async fn get_info(&self) -> JsResult<JsValue> {
//     pub async fn get_info_ex(&self, value: JsValue) -> JsResult<JsValue> {
//         let object: JsValue = if value.is_undefined() { Object::new().into() } else { value.into() };
//         self.get_info_wasm(object).await
//         // self.get_info_wasm(JsValue::default()).await
//         // self.get_block_dag_info_wasm(object.into()).await
//     }

//     // pub async fn get_info(&self, request: JsValue) -> JsResult<JsValue> {
//     //     self.get_info_wasm(request).await
//     // }
// }

build_wrpc_wasm_bindgen_interface!(
    [
        // list of functions with no arguments
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
        // list of functions with `request` argument
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
