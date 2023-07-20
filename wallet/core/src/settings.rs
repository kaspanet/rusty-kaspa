use crate::imports::*;
use crate::result::Result;
use crate::storage::local::Storage;
use dashmap::iter::Iter;
use serde::de::DeserializeOwned;
use std::fmt::Display;
use std::hash::Hash;
use std::str::FromStr;
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
    async fn defaults() -> Vec<(Self, String)> {
        vec![(Self::Server, "127.0.0.1"), (Self::Wallet, "kaspa")].into_iter().map(|(k, v)| (k, v.to_string())).collect()
    }
}

#[async_trait]
pub trait DefaultSettings: Sized {
    async fn defaults() -> Vec<(Self, String)>;
}

#[derive(Debug, Clone)]
pub struct SettingsStore<K>
where
    K: DefaultSettings + Display + Eq + Hash + Clone + Serialize + DeserializeOwned + 'static,
{
    map: DashMap<K, String>,
    storage: Storage,
}

impl<K> SettingsStore<K>
where
    K: DefaultSettings + Display + Eq + Hash + Clone + Serialize + DeserializeOwned + 'static,
{
    pub fn try_new(filename: &str) -> Result<Self> {
        Ok(Self { map: DashMap::default(), storage: Storage::new(filename)? })
    }

    pub fn new_with_storage(storage: Storage) -> Self {
        Self { map: DashMap::default(), storage }
    }

    pub fn get<V>(&self, key: K) -> Option<V>
    where
        V: FromStr,
    {
        let s = self.map.get(&key);
        if let Some(s) = s {
            let s = s.as_str();
            match s.parse() {
                Ok(v) => Some(v),
                Err(_) => {
                    log_error!("Unable to parse setting key `{}` with value `{}`", key, s);
                    None
                }
            }
        } else {
            None
        }
    }

    pub async fn set<V>(&self, key: K, value: V) -> Result<()>
    where
        V: Display,
    {
        let v = value.to_string();

        self.map.insert(key, v);
        self.try_store().await?;
        Ok(())
    }

    pub async fn try_load(&self) -> Result<()> {
        let list: Vec<(K, String)> = if self.storage.exists().await? {
            workflow_store::fs::read_json(self.storage.filename()).await?
        } else {
            <K as DefaultSettings>::defaults().await
        };

        self.map.clear();
        list.into_iter().for_each(|(k, v)| {
            self.map.insert(k, v);
        });
        Ok(())
    }

    pub async fn try_store(&self) -> Result<()> {
        let map = self.map.clone().into_iter().map(|(k, v)| (k, v)).collect::<Vec<_>>();
        workflow_store::fs::write_json(self.storage.filename(), &map).await?;
        Ok(())
    }

    pub fn iter(&self) -> Iter<'_, K, String> {
        self.map.iter()
    }
}
