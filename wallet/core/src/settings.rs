use crate::imports::*;
use crate::result::Result;
use crate::storage::local::Storage;
// use dashmap::iter::Iter;
use serde::de::DeserializeOwned;
use serde_json::{from_value, to_value, Map, Value};
use std::fmt::Display;
use std::hash::Hash;
use std::marker::PhantomData;
// use std::str::FromStr;
use workflow_core::enums::Describe;

#[derive(Describe, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum WalletSettings {
    #[describe("Network type (mainnet|testnet|devnet|simnet)")]
    Network,
    #[describe("Server address (default: 127.0.0.1)")]
    Server,
    #[describe("Wallet storage or file name (default 'kaspa')")]
    Wallet,
}

impl WalletSettings {
    pub fn to_lowercase_string(&self) -> String {
        self.to_string().to_lowercase()
    }
}

#[async_trait]
impl DefaultSettings for WalletSettings {
    async fn defaults() -> Vec<(Self, Value)> {
        vec![(Self::Server, to_value("127.0.0.1").unwrap()), (Self::Wallet, to_value("kaspa").unwrap())]
        // .into_iter().map(|(k, v)| (k, v.to_string())).collect()
    }
}

#[async_trait]
pub trait DefaultSettings: Sized {
    async fn defaults() -> Vec<(Self, Value)>;
}

#[derive(Debug, Clone)]
pub struct SettingsStore<K>
where
    K: DefaultSettings + Display + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    map: DashMap<String, Value>,
    storage: Storage,
    phantom: PhantomData<K>,
}

impl<K> SettingsStore<K>
where
    K: DefaultSettings + Display + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub fn try_new(filename: &str) -> Result<Self> {
        Ok(Self { map: DashMap::default(), storage: Storage::new(filename)?, phantom: PhantomData })
    }

    pub fn new_with_storage(storage: Storage) -> Self {
        Self { map: DashMap::default(), storage, phantom: PhantomData }
    }

    pub fn get<V>(&self, key: K) -> Option<V>
    where
        V: DeserializeOwned,
    {
        let s = self.map.get(&key.to_string());
        if let Some(s) = s {
            match from_value::<V>(s.value().clone()) {
                Ok(v) => Some(v),
                Err(err) => {
                    log_error!("Unable to parse setting key `{key}`: `{err}`");
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
        let v = to_value(value)?;
        self.map.insert(key.to_string(), v);
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

        let list = if list.is_none() {
            Value::Object(Map::from_iter(<K as DefaultSettings>::defaults().await.into_iter().map(|(k, v)| (k.to_string(), v))))
        } else {
            list.unwrap()
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
    K: DefaultSettings + Display + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
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
