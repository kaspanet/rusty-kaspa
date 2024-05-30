//!
//! Multi-platform storage for wallet and application settings.
//!

use crate::imports::*;
use crate::result::Result;
use crate::storage::local::Storage;
use serde::de::DeserializeOwned;
use serde_json::{from_value, to_value, Map, Value};
use std::hash::Hash;
use std::marker::PhantomData;
use std::path::PathBuf;
use workflow_core::enums::Describe;
use workflow_store::fs;

#[derive(Describe, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum WalletSettings {
    #[describe("Network type (mainnet|testnet-10|testnet-11)")]
    Network,
    #[describe("Server address (default: 127.0.0.1)")]
    Server,
    #[describe("Wallet storage or file name (default 'kaspa')")]
    Wallet,
}

#[async_trait]
impl DefaultSettings for WalletSettings {
    async fn defaults() -> Vec<(Self, Value)> {
        vec![(Self::Server, to_value("public").unwrap()), (Self::Wallet, to_value("kaspa").unwrap())]
    }
}

#[async_trait]
pub trait DefaultSettings: Sized {
    async fn defaults() -> Vec<(Self, Value)>;
}

#[derive(Debug, Clone)]
pub struct SettingsStore<K>
where
    K: DefaultSettings + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    map: DashMap<String, Value>,
    storage: Storage,
    phantom: PhantomData<K>,
}

impl<K> SettingsStore<K>
where
    K: DefaultSettings + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub fn try_new(filename: &str) -> Result<Self> {
        Ok(Self { map: DashMap::default(), storage: Storage::try_new(&format!("{filename}.settings"))?, phantom: PhantomData })
    }

    pub fn new_with_storage(storage: Storage) -> Self {
        Self { map: DashMap::default(), storage, phantom: PhantomData }
    }

    pub fn get<V>(&self, key: K) -> Option<V>
    where
        V: DeserializeOwned,
    {
        let ks = to_value(key).unwrap();
        let ks = ks.as_str().expect("Unable to convert key to string");
        let s = self.map.get(ks); //&key.to_string());
        if let Some(s) = s {
            match from_value::<V>(s.value().clone()) {
                Ok(v) => Some(v),
                Err(err) => {
                    log_error!("Unable to parse setting key `{ks}`: `{err}`");
                    None
                }
            }
        } else {
            None
        }
    }

    pub async fn set<V>(&self, key: K, value: V) -> Result<()>
    where
        V: Serialize,
    {
        let ks = to_value(key).unwrap();
        let ks = ks.as_str().expect("Unable to convert key to string");

        let v = to_value(value)?;
        self.map.insert(ks.to_string(), v);
        self.try_store().await?;
        Ok(())
    }

    pub async fn try_load(&self) -> Result<()> {
        let list: Option<Value> = if self.storage.exists().await? {
            let v: Result<Value> = workflow_store::fs::read_json(self.storage.filename()).await.map_err(|err| err.into());
            match v {
                Ok(v) => v.is_object().then_some(v),
                Err(err) => {
                    log_error!("Unable to read settings file: `{err}`");
                    None
                }
            }
        } else {
            None
        };

        let list = if let Some(value) = list {
            value
        } else {
            Value::Object(Map::from_iter(<K as DefaultSettings>::defaults().await.into_iter().map(|(k, v)| {
                let ks = to_value(k).unwrap();
                let ks = ks.as_str().expect("Unable to convert key to string");

                (ks.to_string(), v)
            })))
        };

        self.map.clear();
        if let Value::Object(map) = list {
            map.into_iter().for_each(|(k, v)| {
                self.map.insert(k, v);
            });
        }

        Ok(())
    }

    pub async fn try_store(&self) -> Result<()> {
        let map = Map::from_iter(self.map.clone().into_iter());
        self.storage.ensure_dir().await?;
        workflow_store::fs::write_json(self.storage.filename(), &Value::Object(map)).await?;
        Ok(())
    }
}

#[async_trait]
pub trait SettingsStoreT: Send + Sync + 'static {
    async fn get<V>(&self, key: &str) -> Option<V>
    where
        V: Serialize + DeserializeOwned + Send + Sync + 'static;
    async fn set<V>(&self, key: &str, value: V) -> Result<()>
    where
        V: Serialize + DeserializeOwned + Send + Sync + 'static;
}

#[async_trait]
impl<K> SettingsStoreT for SettingsStore<K>
where
    K: DefaultSettings + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    async fn get<V>(&self, key: &str) -> Option<V>
    where
        V: Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        if let Some(v) = self.map.get(&key.to_string()).map(|v| v.value().clone()) {
            //serde_json::from_str(&v).ok()
            from_value(v).ok()
        } else {
            None
        }
    }
    async fn set<V>(&self, key: &str, value: V) -> Result<()>
    where
        V: Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.map.insert(key.to_string(), to_value(value).unwrap());
        self.try_store().await?;
        Ok(())
    }
}

pub fn application_folder() -> Result<PathBuf> {
    Ok(fs::resolve_path(storage::local::default_storage_folder())?)
}

pub async fn ensure_application_folder() -> Result<()> {
    let path = application_folder()?;
    log_info!("Creating application folder: `{}`", path.display());
    fs::create_dir_all(&path).await?;
    Ok(())
}
