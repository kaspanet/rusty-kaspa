use crate::imports::*;
// use crate::serializable::{numeric,string};
use crate::result::Result;
use kaspa_addresses::Address;
use serde::de::DeserializeOwned;
// use serde::de::DeserializeOwned;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VirtualTransactionT<T>
where
    T: Clone + serde::Serialize,
{
    //} + Deserialize {
    pub version: u32,
    pub generator: Option<String>,
    pub transactions: Vec<T>,
    pub addresses: Option<Vec<Address>>,
}

impl<T> VirtualTransactionT<T>
where
    T: Clone + Serialize,
{
    pub fn deserialize(json: &str) -> Result<Self>
    where
        T: DeserializeOwned,
    {
        Ok(serde_json::from_str(json)?)
    }

    pub fn serialize(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}
