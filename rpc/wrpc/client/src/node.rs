use crate::imports::*;

/// Data structure representing a Node connection endpoint.
/// @category Node RPC
#[derive(Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen(inspectable)]
pub struct Node {
    /// The unique identifier of the node.
    #[wasm_bindgen(getter_with_clone)]
    pub id: String,
    /// The URL of the node WebSocket (wRPC URL).
    #[wasm_bindgen(getter_with_clone)]
    pub url: String,
}

impl Eq for Node {}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

impl Node {
    pub fn url(&self) -> String {
        self.url.clone()
    }
}
