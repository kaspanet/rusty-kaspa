use std::fmt::Display;
use std::str::FromStr;
use dashmap::iter::Iter;
use crate::imports::*;
use crate::result::Result;
use crate::storage::local::Storage;
use workflow_core::enums::Describe;

#[derive(Describe, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum Settings {
    #[describe("Network type (mainnet|testnet|devnet|simnet)")]
    Network,
    #[describe("Server address (default: 127.0.0.1)")]
    Server,
    #[describe("Wallet storage or file name (default 'kaspa'")]
    Wallet,
}

impl Settings {
    pub fn to_lowercase_string(&self) -> String {
        self.to_string().to_lowercase()
    }
}

#[derive(Default, Debug, Clone)]
pub struct SettingsStore {
    map: DashMap<Settings, String>,
}

impl SettingsStore {
    pub fn new(map: DashMap<Settings, String>) -> Self {
        Self { map }
    }

    pub fn get<T>(&self, key: Settings) -> Option<T>
    where
        T: FromStr,
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

    pub async fn set<T>(&self, key: Settings, value: T) -> Result<()>
    where
        T: Display,
    {
        let v = value.to_string();

        self.map.insert(key, v);
        self.try_store().await?;
        Ok(())
    }

    pub async fn try_load(&self) -> Result<()> {
        let storage = Storage::default_settings_store();
        let list: Vec<(Settings, String)> =
            if storage.exists().await? { workflow_store::fs::read_json(storage.filename()).await? } else { vec![] };

        self.map.clear();
        list.into_iter().for_each(|(k, v)| {
            self.map.insert(k, v);
        });
        Ok(())
    }

    pub async fn try_store(&self) -> Result<()> {
        let map = self.map.clone().into_iter().map(|(k, v)| (k, v)).collect::<Vec<_>>();
        let storage = Storage::default_settings_store();
        workflow_store::fs::write_json(storage.filename(), &map).await?;
        Ok(())
    }

    pub fn iter(&self) -> Iter<'_, Settings, String> {
        self.map.iter()
    }
}
