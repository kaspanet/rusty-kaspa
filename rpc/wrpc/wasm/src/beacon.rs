use crate::client::{RpcClient, RpcConfig};
use crate::imports::*;
pub use kaspa_rpc_macros::declare_typescript_wasm_interface as declare;
use kaspa_wrpc_client::node::Node;
use kaspa_wrpc_client::Beacon as NativeBeacon;
use serde::ser;

declare! {
    IBeaconConfig,
    "IBeaconConfig | string[]",
    r#"
    /**
     * RPC Beacon configuration options
     * 
     * @category Node RPC
     */
    export interface IBeaconConfig {
        /**
         * Optional URLs for beacons
         */
        urls?: string[];
    }
    "#,
}

declare! {
    IBeaconConnect,
    "IBeaconConnect | NetworkId | string",
    r#"
    /**
     * RPC Beacon connection options
     * 
     * @category Node RPC
     */
    export interface IBeaconConnect {
        /**
         * RPC encoding: `borsh` (default) or `json`
         */
        encoding?: Encoding | string;
        /**
         * Network identifier: `mainnet` or `testnet-11` etc.
         */
        networkId?: NetworkId | string;
    }
    "#,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconConnect {
    pub encoding: Option<Encoding>,
    pub network_id: NetworkId,
}

impl TryFrom<IBeaconConnect> for BeaconConnect {
    type Error = Error;
    fn try_from(config: IBeaconConnect) -> Result<Self> {
        if let Ok(network_id) = NetworkId::try_from(&config.clone().into()) {
            Ok(Self { encoding: None, network_id })
        } else {
            Ok(serde_wasm_bindgen::from_value(config.into())?) //.map_err(Into::into)
        }
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Array, typescript_type = "string[]")]
    pub type IBeaconArray;
}

/// @category Node RPC
#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct Beacon {
    beacon: Arc<NativeBeacon>,
}

#[wasm_bindgen]
impl Beacon {
    #[wasm_bindgen(constructor)]
    pub fn ctor(args: Option<IBeaconConfig>) -> Result<Beacon> {
        if let Some(args) = args {
            Ok(Self { beacon: Arc::new(NativeBeacon::try_from(args)?) })
        } else {
            Ok(Self { beacon: Arc::new(NativeBeacon::default()) })
        }
    }
}

#[wasm_bindgen]
impl Beacon {
    #[wasm_bindgen(js_name = getNode)]
    pub async fn get_node(&self, encoding: Encoding, network_id: INetworkId) -> Result<Node> {
        self.beacon.get_node(encoding, network_id.try_into()?).await
    }

    #[wasm_bindgen(js_name = getUrl)]
    pub async fn get_url(&self, encoding: Encoding, network_id: INetworkId) -> Result<String> {
        self.beacon.get_url(encoding, network_id.try_into()?).await
    }

    pub async fn connect(&self, options: IBeaconConnect) -> Result<RpcClient> {
        let BeaconConnect { encoding, network_id } = options.try_into()?;
        let encoding = encoding.unwrap_or(Encoding::Borsh);
        let node = self.beacon.get_node(encoding, network_id).await?;

        let config = RpcConfig { url: Some(node.url), encoding, network_id: Some(network_id) };

        let client = RpcClient::new(Some(config))?;
        client.connect(None).await?;
        Ok(client)
    }
}

impl TryFrom<IBeaconConfig> for NativeBeacon {
    type Error = Error;
    fn try_from(_config: IBeaconConfig) -> Result<Self> {
        // TODO
        Ok(NativeBeacon::default())
        // let urls = arrayZ
        //     .iter()
        //     .map(|url| {
        //         if let Some(url) = url.as_string() {
        //             Ok(url)
        //         } else {
        //             Err(Error::Custom("Invalid argument: expecting an array of strings".to_string()))
        //         }
        //     })
        //     .collect::<Result<Vec<String>>>()?;

        // Ok(NativeBeacon::new(urls))
    }
}
