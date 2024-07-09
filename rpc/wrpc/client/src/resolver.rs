use std::sync::OnceLock;

use crate::error::Error;
use crate::imports::*;
use crate::node::NodeDescriptor;
pub use futures::future::join_all;
use rand::seq::SliceRandom;
use rand::thread_rng;
use workflow_core::runtime;
use workflow_http::get_json;

const DEFAULT_VERSION: usize = 2;
const RESOLVERS_PER_FQDN: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolverRecord {
    pub url: String,
    pub enable: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolverConfig {
    resolver: Vec<ResolverRecord>,
}

fn try_parse_resolvers(toml: &str) -> Result<Vec<String>> {
    Ok(toml::from_str::<ResolverConfig>(toml)?
        .resolver
        .into_iter()
        .filter_map(|resolver| resolver.enable.unwrap_or(true).then_some(resolver.url))
        .collect::<Vec<_>>())
}

fn transform(source_list: Vec<String>) -> Vec<Arc<String>> {
    let mut resolvers = vec![];
    for url in source_list.into_iter() {
        if url.contains('*') {
            for n in 0..RESOLVERS_PER_FQDN {
                resolvers.push(Arc::new(url.replace('*', &format!("r{n}"))));
            }
        } else {
            resolvers.push(Arc::new(url));
        }
    }

    resolvers
}

#[derive(Debug)]
struct Inner {
    pub urls: Vec<Arc<String>>,
}

impl Inner {
    pub fn new(urls: Vec<Arc<String>>) -> Self {
        Self { urls }
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
        let toml = include_str!("../Resolvers.toml");
        let urls = try_parse_resolvers(toml).expect("TOML: Unable to parse RPC Resolver list");
        Self { inner: Arc::new(Inner::new(transform(urls))) }
    }
}

impl Resolver {
    pub fn new(urls: Vec<Arc<String>>) -> Self {
        if urls.is_empty() {
            panic!("Resolver: Empty URL list supplied to the constructor.");
        }

        Self { inner: Arc::new(Inner::new(urls)) }
    }

    pub fn urls(&self) -> Vec<Arc<String>> {
        self.inner.urls.clone()
    }

    fn make_url(&self, url: &str, encoding: Encoding, network_id: NetworkId) -> String {
        static SSL: OnceLock<bool> = OnceLock::new();

        let ssl = *SSL.get_or_init(|| {
            if runtime::is_web() {
                js_sys::Reflect::get(&js_sys::global(), &"location".into())
                    .and_then(|location| js_sys::Reflect::get(&location, &"protocol".into()))
                    .ok()
                    .and_then(|protocol| protocol.as_string())
                    .map(|protocol| protocol.starts_with("https"))
                    .unwrap_or(false)
            } else {
                false
            }
        });

        if ssl {
            format!("{}/v{}/rk/wrpc/tls/{}/{}", url, DEFAULT_VERSION, encoding, network_id)
        } else {
            format!("{}/v{}/rk/wrpc/any/{}/{}", url, DEFAULT_VERSION, encoding, network_id)
        }
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

    pub async fn fetch_all(&self, encoding: Encoding, network_id: NetworkId) -> Result<Vec<NodeDescriptor>> {
        let futures = self.inner.urls.iter().map(|url| self.fetch_node_info(url, encoding, network_id)).collect::<Vec<_>>();
        let mut errors = Vec::default();
        let result = join_all(futures)
            .await
            .into_iter()
            .filter_map(|result| match result {
                Ok(node) => Some(node),
                Err(error) => {
                    errors.push(format!("{:?}", error));
                    None
                }
            })
            .collect::<Vec<_>>();
        if result.is_empty() {
            Err(Error::Custom(format!("Failed to connect: {:?}", errors)))
        } else {
            Ok(result)
        }
    }

    pub async fn get_node(&self, encoding: Encoding, network_id: NetworkId) -> Result<NodeDescriptor> {
        self.fetch(encoding, network_id).await
    }

    pub async fn get_url(&self, encoding: Encoding, network_id: NetworkId) -> Result<String> {
        let nodes = self.fetch(encoding, network_id).await?;
        Ok(nodes.url.clone())
    }
}
