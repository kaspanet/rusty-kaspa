use std::sync::OnceLock;

use crate::error::Error;
use crate::imports::*;
use crate::node::NodeDescriptor;
pub use futures::future::join_all;
use rand::seq::SliceRandom;
use rand::thread_rng;
use workflow_core::runtime;
use workflow_http::get_json;

const CURRENT_VERSION: usize = 2;
const RESOLVER_CONFIG: &str = include_str!("../Resolvers.toml");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolverRecord {
    pub address: String,
    pub enable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolverGroup {
    pub template: String,
    pub nodes: Vec<String>,
    pub enable: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolverConfig {
    #[serde(rename = "group")]
    groups: Vec<ResolverGroup>,
    #[serde(rename = "resolver")]
    resolvers: Vec<ResolverRecord>,
}

fn try_parse_resolvers(toml: &str) -> Result<Vec<Arc<String>>> {
    let config = toml::from_str::<ResolverConfig>(toml)?;

    let mut resolvers = config
        .resolvers
        .into_iter()
        .filter_map(|resolver| resolver.enable.unwrap_or(true).then_some(resolver.address))
        .collect::<Vec<_>>();

    let groups = config.groups.into_iter().filter(|group| group.enable.unwrap_or(true)).collect::<Vec<_>>();

    for group in groups {
        let ResolverGroup { template, nodes, .. } = group;
        for node in nodes {
            resolvers.push(template.replace('*', &node));
        }
    }

    Ok(resolvers.into_iter().map(Arc::new).collect::<Vec<_>>())
}

#[derive(Debug)]
struct Inner {
    pub urls: Vec<Arc<String>>,
    pub tls: bool,
    public: bool,
}

impl Inner {
    pub fn new(urls: Option<Vec<Arc<String>>>, tls: bool) -> Self {
        if urls.as_ref().is_some_and(|urls| urls.is_empty()) {
            panic!("Resolver: Empty URL list supplied to the constructor.");
        }

        let mut public = false;
        let urls = urls.unwrap_or_else(|| {
            public = true;
            try_parse_resolvers(RESOLVER_CONFIG).expect("TOML: Unable to parse RPC Resolver list")
        });

        Self { urls, tls, public }
    }
}

///
/// Resolver is a client for obtaining public Kaspa wRPC endpoints.
///
#[derive(Debug, Clone)]
pub struct Resolver {
    inner: Arc<Inner>,
}

impl Default for Resolver {
    fn default() -> Self {
        Self { inner: Arc::new(Inner::new(None, false)) }
    }
}

impl Resolver {
    pub fn new(urls: Option<Vec<Arc<String>>>, tls: bool) -> Self {
        Self { inner: Arc::new(Inner::new(urls, tls)) }
    }

    pub fn urls(&self) -> Option<Vec<Arc<String>>> {
        if self.inner.public {
            None
        } else {
            Some(self.inner.urls.clone())
        }
    }

    pub fn tls(&self) -> bool {
        self.inner.tls
    }

    pub fn tls_as_str(&self) -> &'static str {
        if self.inner.tls {
            "tls"
        } else {
            "any"
        }
    }

    fn make_url(&self, url: &str, encoding: Encoding, network_id: NetworkId) -> String {
        static TLS: OnceLock<&'static str> = OnceLock::new();

        let tls = *TLS.get_or_init(|| {
            if runtime::is_web() {
                let tls = js_sys::Reflect::get(&js_sys::global(), &"location".into())
                    .and_then(|location| js_sys::Reflect::get(&location, &"protocol".into()))
                    .ok()
                    .and_then(|protocol| protocol.as_string())
                    .map(|protocol| protocol.starts_with("https"))
                    .unwrap_or(false);
                if tls {
                    "tls"
                } else {
                    self.tls_as_str()
                }
            } else {
                self.tls_as_str()
            }
        });

        format!("{url}/v{CURRENT_VERSION}/kaspa/{network_id}/{tls}/wrpc/{encoding}")
    }

    async fn fetch_node_info(&self, url: &str, encoding: Encoding, network_id: NetworkId) -> Result<NodeDescriptor> {
        let url = self.make_url(url, encoding, network_id);
        let node =
            get_json::<NodeDescriptor>(&url).await.map_err(|error| Error::custom(format!("Unable to connect to {url}: {error}")))?;
        Ok(node)
    }

    pub async fn fetch(&self, encoding: Encoding, network_id: NetworkId) -> Result<NodeDescriptor> {
        let mut urls = self.inner.urls.clone();
        urls.shuffle(&mut thread_rng());

        let mut errors = Vec::default();
        for url in urls {
            match self.fetch_node_info(&url, encoding, network_id).await {
                Ok(node) => return Ok(node),
                Err(error) => errors.push(error),
            }
        }
        Err(Error::Custom(format!("Failed to connect: {:?}", errors)))
    }

    pub async fn get_node(&self, encoding: Encoding, network_id: NetworkId) -> Result<NodeDescriptor> {
        self.fetch(encoding, network_id).await
    }

    pub async fn get_url(&self, encoding: Encoding, network_id: NetworkId) -> Result<String> {
        let nodes = self.fetch(encoding, network_id).await?;
        Ok(nodes.url.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_config_1() {
        let toml = r#"
            [[group]]
            enable = true
            template = "https://*.example.org"
            nodes = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"]

            [[group]]
            enable = true
            template = "https://*.example.com"
            nodes = ["iota", "kappa", "lambda", "mu", "nu", "xi", "omicron", "pi"]

            [[resolver]]
            enable = true
            address = "http://127.0.0.1:8888"
        "#;

        let urls = try_parse_resolvers(toml).expect("TOML: Unable to parse RPC Resolver list");
        // println!("{:#?}", urls);
        assert_eq!(urls.len(), 17);
    }

    #[test]
    fn test_resolver_config_2() {
        let _urls = try_parse_resolvers(RESOLVER_CONFIG).expect("TOML: Unable to parse RPC Resolver list");
        // println!("{:#?}", urls);
    }
}
