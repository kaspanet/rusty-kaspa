use crate::imports::*;
use xxhash_rust::xxh3::xxh3_64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    #[serde(skip)]
    pub id: String,
    pub name: Option<String>,
    pub location: Option<String>,
    pub address: String,
    pub transport: Transport,
    pub encoding: WrpcEncoding,
    pub network: NetworkId,
    pub enable: Option<bool>,
    pub bias: Option<f64>,
    pub version: Option<String>,
    pub provider: Option<String>,
    pub link: Option<String>,
}

impl Eq for Node {}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let title = self.name.clone().unwrap_or(self.address.to_string());
        write!(f, "{}", title)
    }
}

impl Node {
    pub fn params(&self) -> Params {
        Params::new(self.encoding, self.network)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    node: Vec<Node>,
}

pub fn try_parse_nodes(toml: &str) -> Result<Vec<Arc<Node>>> {
    let nodes: Vec<Arc<Node>> = toml::from_str::<NodeConfig>(toml)?
        .node
        .into_iter()
        .filter_map(|mut node| {
            let id = xxh3_64(node.address.as_bytes());
            node.id = format!("{id:x}")[0..8].to_string();
            node.enable.unwrap_or(true).then_some(node).map(Arc::new)
        })
        .collect::<Vec<_>>();
    Ok(nodes)
}

impl AsRef<Node> for Node {
    fn as_ref(&self) -> &Node {
        self
    }
}
