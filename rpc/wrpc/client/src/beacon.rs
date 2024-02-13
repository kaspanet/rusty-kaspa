use crate::error::Error;
use crate::imports::*;
use crate::nodes::Node;
use js_sys::Object;
use workflow_http::get_json;
use workflow_wasm::extensions::ObjectExtension;

const DEFAULT_VERSION: usize = 1;
const DEFAULT_BEACONS: &[(&str, usize)] = &[("https://beacon.kaspa-ng.org", DEFAULT_VERSION)];

#[wasm_bindgen(typescript_custom_section)]
const TS_BEACON: &str = r#"
/**
 * 
 * 
 * @category Node RPC
 */
export interface IBeacon {
    url : string;
    version? : number;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Object, typescript_type = "IBeacon")]
    pub type IBeacon;
}

#[wasm_bindgen(typescript_custom_section)]
const TS_BEACON_AGGREGATOR: &'static str = r#"
/**
 * 
 * 
 * @category Node RPC
 */
export interface IBeaconAggregator {
    beacons : IBeacon[];
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Object, typescript_type = "IBeaconAggregator | StringArray | undefined")]
    pub type IBeaconAggregator;
}

/// @category Node RPC
#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct Beacon {
    #[wasm_bindgen(getter_with_clone)]
    pub url: String,
    pub version: Option<usize>,
}

#[wasm_bindgen]
impl Beacon {
    #[wasm_bindgen(constructor)]
    pub fn ctor(args: IBeacon) -> Result<Beacon> {
        Beacon::try_from(JsValue::from(args))
    }
}

impl Beacon {
    pub fn new(url: String, version: Option<usize>) -> Self {
        Self { url, version }
    }

    pub fn address(&self) -> &str {
        self.url.as_str()
    }

    pub async fn get_node(&self, encoding: Encoding, network_id: NetworkId) -> Result<Option<Node>> {
        let url = format!("{}/v{}/node/{}/{}", self.url, self.version.unwrap_or(DEFAULT_VERSION), encoding, network_id);
        let node = get_json::<Node>(url).await?;
        Ok(Some(node))
    }

    pub async fn get_all(&self, encoding: Encoding, network_id: Option<NetworkId>) -> Result<Vec<Node>> {
        if let Some(network_id) = network_id {
            let url = format!("{}/v{}/list/{}/{}", self.url, self.version.unwrap_or(DEFAULT_VERSION), encoding, network_id);
            let nodes = get_json::<Vec<Node>>(url).await?;
            Ok(nodes)
        } else {
            let url = format!("{}/v{}/list/{}/", self.url, self.version.unwrap_or(DEFAULT_VERSION), encoding);
            let nodes = get_json::<Vec<Node>>(url).await?;
            Ok(nodes)
        }
    }
}

impl TryFrom<JsValue> for Beacon {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self> {
        if let Some(url) = js_value.as_string() {
            Ok(Self { url, version: None })
        } else if let Some(object) = Object::try_from(&js_value) {
            let url = object.get_string("url")?;
            let version = object.get_u32("version").map(|v| v as usize).ok();
            Ok(Self { url, version })
        } else {
            Err(Error::Custom("Invalid beacon".to_string()))
        }
    }
}

/// @category Node RPC
#[wasm_bindgen]
pub struct BeaconAggregator {
    // #[wasm_bindgen(skip)]
    #[wasm_bindgen(getter_with_clone)]
    pub beacons: Vec<Beacon>,
}

impl Default for BeaconAggregator {
    fn default() -> Self {
        Self { beacons: DEFAULT_BEACONS.iter().map(|(url, version)| Beacon::new(url.to_string(), Some(*version))).collect() }
    }
}

cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        use kaspa_consensus_wasm::StringArrayOrNone;

        #[wasm_bindgen]
        impl BeaconAggregator {
            #[wasm_bindgen(constructor)]
            pub fn ctor(beacons: StringArrayOrNone) -> Result<BeaconAggregator> {
                if beacons.is_undefined() || beacons.is_null() {
                    Ok(Default::default())
                } else if beacons.is_array() {
                    let array = js_sys::Array::unchecked_from_js(beacons.into());
                    let beacons = array.iter().map(Beacon::try_from).collect::<Result<Vec<Beacon>>>()?;
                    Ok(Self { beacons })
                } else {
                    Err(Error::custom("Supplied argument must be an array"))
                }
            }
        }
    }
}
// #[cfg_attr(sdk,wasm_bindgen)]
// impl BeaconAggregator {
//     #[cfg_attr(sdk,wasm_bindgen(constructor))]
//     pub fn ctor(beacons: StringArrayOrNone) -> Result<BeaconAggregator> {
//         if beacons.is_undefined() || beacons.is_null() {
//             Ok(Default::default())
//         } else if beacons.is_array() {
//             let array = Array::unchecked_from_js(beacons.into());
//             let beacons = array.iter().map(Beacon::try_from).collect::<Result<Vec<Beacon>>>()?;
//             Ok(Self { beacons })
//         } else {
//             Err(Error::custom("Supplied argument must be an array"))
//         }
//     }
// }

impl BeaconAggregator {
    pub fn new(beacons: Vec<Beacon>) -> Self {
        Self { beacons }
    }

    // TODO - enumerate all nodes, select one from global list...
    pub async fn get_node(&self, encoding: Encoding, network_id: NetworkId) -> Result<Option<Node>> {
        for beacon in &self.beacons {
            if let Ok(Some(node)) = beacon.get_node(encoding, network_id).await {
                return Ok(Some(node));
            }
        }
        Ok(None)
    }
}
