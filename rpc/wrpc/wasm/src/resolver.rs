use crate::client::{RpcClient, RpcConfig};
use crate::imports::*;
use js_sys::Array;
pub use kaspa_rpc_macros::declare_typescript_wasm_interface as declare;
use kaspa_wrpc_client::node::NodeDescriptor;
use kaspa_wrpc_client::Resolver as NativeResolver;
use serde::ser;
use workflow_wasm::extensions::ObjectExtension;

declare! {
    IResolverConfig,
    "IResolverConfig | string[]",
    r#"
    /**
     * RPC Resolver configuration options
     * 
     * @category Node RPC
     */
    export interface IResolverConfig {
        /**
         * Optional URLs for one or multiple resolvers.
         */
        urls?: string[];
    }
    "#,
}

declare! {
    IResolverConnect,
    "IResolverConnect | NetworkId | string",
    r#"
    /**
     * RPC Resolver connection options
     * 
     * @category Node RPC
     */
    export interface IResolverConnect {
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
pub struct ResolverConnect {
    pub encoding: Option<Encoding>,
    pub network_id: NetworkId,
}

impl TryFrom<IResolverConnect> for ResolverConnect {
    type Error = Error;
    fn try_from(config: IResolverConnect) -> Result<Self> {
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
    pub type IResolverArray;
}

///
/// Resolver is a client for obtaining public Kaspa wRPC endpoints.
///
/// @see {@link IResolverConfig}, {@link IResolverConnect}, {@link RpcClient}
/// @category Node RPC
///
#[derive(Debug, Clone)]
#[wasm_bindgen(inspectable)]
pub struct Resolver {
    resolver: NativeResolver,
}

impl Resolver {
    pub fn new(resolver: NativeResolver) -> Self {
        Self { resolver }
    }
}

#[wasm_bindgen]
impl Resolver {
    /// Creates a new Resolver client with the given
    /// configuration supplied as {@link IResolverConfig}
    /// interface. If not supplied, the default configuration
    /// containing a list of community-operated resolvers
    /// will be used.
    #[wasm_bindgen(constructor)]
    pub fn ctor(args: Option<IResolverConfig>) -> Result<Resolver> {
        if let Some(args) = args {
            Ok(Self { resolver: NativeResolver::try_from(args)? })
        } else {
            Ok(Self { resolver: NativeResolver::default() })
        }
    }
}

#[wasm_bindgen]
impl Resolver {
    /// List of public Kaspa Beacon URLs.
    /// @see {@link IBeaconArray}
    #[wasm_bindgen(getter)]
    pub fn urls(&self) -> IResolverArray {
        Array::from_iter(self.resolver.urls().iter().map(|v| JsValue::from(v.as_str()))).unchecked_into()
    }

    /// Fetches a public Kaspa wRPC endpoint for the given encoding and network identifier.
    /// @see {@link Encoding}, {@link NetworkId}, {@link Node}
    #[wasm_bindgen(js_name = getNode)]
    pub async fn get_node(&self, encoding: Encoding, network_id: INetworkId) -> Result<NodeDescriptor> {
        self.resolver.get_node(encoding, network_id.try_into()?).await
    }

    /// Fetches a public Kaspa wRPC endpoint URL for the given encoding and network identifier.
    /// @see {@link Encoding}, {@link NetworkId}
    #[wasm_bindgen(js_name = getUrl)]
    pub async fn get_url(&self, encoding: Encoding, network_id: INetworkId) -> Result<String> {
        self.resolver.get_url(encoding, network_id.try_into()?).await
    }

    /// Connect to a public Kaspa wRPC endpoint for the given encoding and network identifier
    /// supplied via {@link IResolverConnect} interface.
    /// @see {@link IResolverConnect}, {@link RpcClient}
    pub async fn connect(&self, options: IResolverConnect) -> Result<RpcClient> {
        let ResolverConnect { encoding, network_id } = options.try_into()?;
        let config = RpcConfig { resolver: Some(self.clone()), url: None, encoding, network_id: Some(network_id) };
        let client = RpcClient::new(Some(config))?;
        client.connect(None).await?;
        Ok(client)
    }
}

impl TryFrom<IResolverConfig> for NativeResolver {
    type Error = Error;
    fn try_from(config: IResolverConfig) -> Result<Self> {
        if let Ok(urls) = config.get_vec("urls") {
            let urls = urls.into_iter().map(|v| v.as_string()).collect::<Option<Vec<_>>>().ok_or(Error::custom("Invalid URL"))?;
            Ok(NativeResolver::new(urls.into_iter().map(Arc::new).collect()))
        } else {
            Ok(NativeResolver::default())
        }
    }
}

impl TryFrom<JsValue> for Resolver {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self> {
        Ok(Resolver::try_from_js_value(js_value)?)
    }
}

impl From<Resolver> for NativeResolver {
    fn from(resolver: Resolver) -> Self {
        resolver.resolver
    }
}

impl From<NativeResolver> for Resolver {
    fn from(resolver: NativeResolver) -> Self {
        Self { resolver }
    }
}
