use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter, StoreError, DB};
use kaspa_database::prelude::CachePolicy;
use kaspa_hashes::Hash;
use std::sync::Arc;
use std::fmt::Display;
use rocksdb::WriteBatch;

pub type ContractAddress = Hash;
pub type StateKey = Vec<u8>;
pub type StateValue = Vec<u8>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ContractStateKey {
    pub contract_address: ContractAddress,
    pub state_key: StateKey,
}

impl ContractStateKey {
    pub fn new(contract_address: ContractAddress, state_key: StateKey) -> Self {
        Self { contract_address, state_key }
    }
    
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.contract_address.as_bytes());
        bytes.extend_from_slice(&self.state_key);
        bytes
    }
    
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 32 {
            return Err("Invalid contract state key length");
        }
        
        let contract_address = Hash::from_slice(&bytes[..32]);
        let state_key = bytes[32..].to_vec();
        
        Ok(Self { contract_address, state_key })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DbContractStateKey(Vec<u8>);

impl DbContractStateKey {
    pub fn new(contract_state_key: &ContractStateKey) -> Self {
        Self(contract_state_key.to_bytes())
    }
}

impl AsRef<[u8]> for DbContractStateKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for DbContractStateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02x?}", self.0)
    }
}

impl Display for ContractStateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:?}", self.contract_address, self.state_key)
    }
}

pub trait ContractStateStoreReader {
    fn get(&self, key: &ContractStateKey) -> Result<Option<StateValue>, StoreError>;
    fn get_all_for_contract(&self, contract_address: &ContractAddress) -> Result<Vec<(StateKey, StateValue)>, StoreError>;
}

pub trait ContractStateStore: ContractStateStoreReader {
    fn set(&mut self, key: &ContractStateKey, value: StateValue) -> Result<(), StoreError>;
    fn delete(&mut self, key: &ContractStateKey) -> Result<(), StoreError>;
    fn set_many(&mut self, entries: &[(ContractStateKey, StateValue)]) -> Result<(), StoreError>;
}

#[derive(Clone)]
pub struct DbContractStateStore {
    db: Arc<DB>,
    access: CachedDbAccess<DbContractStateKey, StateValue>,
}

impl DbContractStateStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy, prefix: Vec<u8>) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db, cache_policy, prefix),
        }
    }
    
    pub fn write_batch(&mut self, batch: &mut WriteBatch, entries: &[(ContractStateKey, StateValue)]) -> Result<(), StoreError> {
        let mut writer = BatchDbWriter::new(batch);
        for (key, value) in entries {
            self.access.write(&mut writer, DbContractStateKey::new(key), value.clone())?;
        }
        Ok(())
    }
    
    pub fn delete_batch(&mut self, batch: &mut WriteBatch, keys: &[ContractStateKey]) -> Result<(), StoreError> {
        let mut writer = BatchDbWriter::new(batch);
        for key in keys {
            self.access.delete(&mut writer, DbContractStateKey::new(key))?;
        }
        Ok(())
    }
}

impl ContractStateStoreReader for DbContractStateStore {
    fn get(&self, key: &ContractStateKey) -> Result<Option<StateValue>, StoreError> {
        match self.access.read(DbContractStateKey::new(key)) {
            Ok(value) => Ok(Some(value)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
    
    fn get_all_for_contract(&self, contract_address: &ContractAddress) -> Result<Vec<(StateKey, StateValue)>, StoreError> {
        let prefix = contract_address.as_bytes().to_vec();
        let mut results = Vec::new();
        
        for item in self.access.iterator() {
            let (key_bytes, value) = match item {
                Ok((k, v)) => (k, v),
                Err(_) => continue,
            };
            if key_bytes.starts_with(&prefix) {
                let state_key = key_bytes[32..].to_vec();
                results.push((state_key, value));
            }
        }
        
        Ok(results)
    }
}

impl ContractStateStore for DbContractStateStore {
    fn set(&mut self, key: &ContractStateKey, value: StateValue) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db);
        self.access.write(&mut writer, DbContractStateKey::new(key), value)
    }
    
    fn delete(&mut self, key: &ContractStateKey) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db);
        self.access.delete(&mut writer, DbContractStateKey::new(key))
    }
    
    fn set_many(&mut self, entries: &[(ContractStateKey, StateValue)]) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db);
        for (key, value) in entries {
            self.access.write(&mut writer, DbContractStateKey::new(key), value.clone())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_contract_state_key_serialization() {
        let contract_address = Hash::from_u64_word(12345);
        let state_key = b"test_key".to_vec();
        
        let key = ContractStateKey::new(contract_address, state_key.clone());
        let bytes = key.to_bytes();
        let restored_key = ContractStateKey::from_bytes(&bytes).unwrap();
        
        assert_eq!(key.contract_address, restored_key.contract_address);
        assert_eq!(key.state_key, restored_key.state_key);
    }
}
