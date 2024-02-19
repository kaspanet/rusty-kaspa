use crate::error::Error;
use crate::imports::*;
use crate::node::NodeDescriptor;
pub use futures::future::join_all;
use rand::seq::SliceRandom;
use rand::thread_rng;
use workflow_http::get_json;

const DEFAULT_VERSION: usize = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconRecord {
    pub url: String,
    pub enable: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BeaconConfig {
    beacon: Vec<BeaconRecord>,
}

fn try_parse_beacons(toml: &str) -> Result<Vec<Arc<String>>> {
    Ok(toml::from_str::<BeaconConfig>(toml)?
        .beacon
        .into_iter()
        .filter_map(|beacon| beacon.enable.unwrap_or(true).then_some(Arc::new(beacon.url)))
        .collect::<Vec<_>>())
}

///
/// Beacon is a client for obtaining public Kaspa wRPC endpoints.
///
#[derive(Debug, Clone)]
pub struct Beacon {
    pub urls: Vec<Arc<String>>,
}

impl Default for Beacon {
    fn default() -> Self {
        let toml = include_str!("../Beacons.toml");
        let urls = try_parse_beacons(toml).expect("TOML: Unable to parse RPC Beacon list");
        Self { urls }
    }
}

impl Beacon {
    pub fn new(urls: Vec<Arc<String>>) -> Self {
        Self { urls }
    }

    async fn fetch_node_info(&self, url: &str, encoding: Encoding, network_id: NetworkId) -> Result<NodeDescriptor> {
        let url = format!("{}/v{}/wrpc/{}/{}", url, DEFAULT_VERSION, encoding, network_id);
        let node =
            get_json::<NodeDescriptor>(&url).await.map_err(|error| Error::custom(format!("Unable to connect to {url}: {error}")))?;
        Ok(node)
    }

    pub async fn fetch(&self, encoding: Encoding, network_id: NetworkId) -> Result<NodeDescriptor> {
        let mut urls = self.urls.clone();
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
        let futures = self.urls.iter().map(|url| self.fetch_node_info(url, encoding, network_id)).collect::<Vec<_>>();
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
