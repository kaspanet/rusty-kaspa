use crate::imports::*;

///
/// Data structure representing a Node connection endpoint
/// as provided by the {@link Resolver}.
///
/// @category Node RPC
///
#[derive(Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen(inspectable)]
pub struct NodeDescriptor {
    /// The unique identifier of the node.
    #[wasm_bindgen(getter_with_clone)]
    pub id: String,
    /// The URL of the node WebSocket (wRPC URL).
    #[wasm_bindgen(getter_with_clone)]
    pub url: String,
    /// Optional name of the node provider.
    #[wasm_bindgen(getter_with_clone)]
    pub provider_name: Option<String>,
    /// Optional site URL of the node provider.
    #[wasm_bindgen(getter_with_clone)]
    pub provider_url: Option<String>,
}

impl Eq for NodeDescriptor {}

impl PartialEq for NodeDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl std::fmt::Display for NodeDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

impl NodeDescriptor {
    pub fn url(&self) -> String {
        self.url.clone()
    }
}
