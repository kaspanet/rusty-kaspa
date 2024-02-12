use crate::imports::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub name: Option<String>,
    pub ident: Option<String>,
    pub location: Option<String>,
    pub protocol: WrpcEncoding,
    pub address: String,
    pub enable: Option<bool>,
    pub link: Option<String>,
    pub network: NetworkId,
    pub bias: Option<f32>,
    pub manual: Option<bool>,
    pub version: Option<String>,
}

impl Eq for Node {}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut title = self.name.clone().unwrap_or(self.address.to_string());
        if let Some(ident) = self.ident.as_ref() {
            title += format!(" ({ident})").as_str();
        } else if let Some(location) = self.location.as_ref() {
            title += format!(" ({location})").as_str();
        }

        write!(f, "{}", title)
    }
}

impl Node {
    // pub fn address(&self) -> String {
    //     self.address.clone()
    // }

    // pub fn wrpc_encoding(&self) -> WrpcEncoding {
    //     self.protocol
    // }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    node: Vec<Node>,
}

pub fn try_parse_nodes(toml: &str) -> Result<Vec<Arc<Node>>> {
    let nodes: Vec<Arc<Node>> = toml::from_str::<NodeConfig>(toml)?
        .node
        .into_iter()
        .filter_map(|node| node.enable.unwrap_or(true).then_some(node).map(Arc::new))
        .collect::<Vec<_>>();
    Ok(nodes)
}
