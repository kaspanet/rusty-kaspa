use crate::imports::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub name: Option<String>,
    pub ident: Option<String>,
    pub location: Option<String>,
    pub protocol: WrpcEncoding,
    pub address: String,
    // pub enable: Option<bool>,
    pub link: Option<String>,
    pub network: NetworkId,
    // pub bias: Option<f32>,
    // pub manual: Option<bool>,
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
    pub fn address(&self) -> String {
        self.address.clone()
    }

    pub fn wrpc_encoding(&self) -> WrpcEncoding {
        self.protocol
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    server: Vec<Node>,
}

// fn try_parse_servers(toml: &str) -> Result<Arc<HashMap<NetworkId, Vec<Node>>>> {
//     let servers: Vec<Node> = toml::from_str::<ServerConfig>(toml)?
//         .server
//         .into_iter()
//         .filter(|server| server.enable.unwrap_or(true))
//         .collect::<Vec<_>>();

//     let servers = HashMap::group_from(servers.into_iter().map(|server| (server.network, server)));

//     Ok(servers.into())
// }

// // fn parse_servers(toml: &str) -> Arc<Vec<Server>> {
// fn parse_servers(toml: &str) -> Arc<HashMap<Network, Vec<Node>>> {
//     match try_parse_servers(toml) {
//         Ok(servers) => servers,
//         Err(e) => {
//             cfg_if! {
//                 if #[cfg(not(target_arch = "wasm32"))] {
//                     println!();
//                     panic!("Error parsing Servers.toml: {}", e);
//                 } else {
//                     log_error!("Error parsing Servers.toml: {}", e);
//                     HashMap::default().into()
//                 }
//             }
//         }
//     }
// }
