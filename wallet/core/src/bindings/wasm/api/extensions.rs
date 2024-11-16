use crate::imports::*;
use js_sys::Object;
use kaspa_consensus_core::Hash;

pub trait WalletApiObjectExtension {
    fn get_secret(&self, key: &str) -> Result<Secret>;
    fn try_get_secret(&self, key: &str) -> Result<Option<Secret>>;
    fn get_network_id(&self, key: &str) -> Result<NetworkId>;
    fn try_get_prv_key_data_id(&self, key: &str) -> Result<Option<PrvKeyDataId>>;
    fn get_prv_key_data_id(&self, key: &str) -> Result<PrvKeyDataId>;
    fn get_account_id(&self, key: &str) -> Result<AccountId>;
    fn try_get_account_id_list(&self, key: &str) -> Result<Option<Vec<AccountId>>>;
    fn get_transaction_id(&self, key: &str) -> Result<Hash>;
}

impl WalletApiObjectExtension for Object {
    fn get_secret(&self, key: &str) -> Result<Secret> {
        let string = self.get_value(key)?.as_string().ok_or(Error::InvalidArgument(key.to_string())).map(|s| s.trim().to_string())?;
        if string.is_empty() {
            Err(Error::SecretIsEmpty(key.to_string()))
        } else {
            Ok(Secret::from(string))
        }
    }

    fn try_get_secret(&self, key: &str) -> Result<Option<Secret>> {
        let string = self.try_get_value(key)?.and_then(|value| value.as_string());
        if let Some(string) = string {
            if string.is_empty() {
                Err(Error::SecretIsEmpty(key.to_string()))
            } else {
                Ok(Some(Secret::from(string)))
            }
        } else {
            Ok(None)
        }
    }

    fn get_network_id(&self, key: &str) -> Result<NetworkId> {
        let value = self.get_value(key)?;
        Ok(NetworkId::try_from(value)?)
    }

    fn try_get_prv_key_data_id(&self, key: &str) -> Result<Option<PrvKeyDataId>> {
        if let Some(value) = self.try_get_value(key)? {
            Ok(Some(PrvKeyDataId::try_from(&value)?))
        } else {
            Ok(None)
        }
    }

    fn get_prv_key_data_id(&self, key: &str) -> Result<PrvKeyDataId> {
        PrvKeyDataId::try_from(&self.get_value(key)?)
    }

    fn get_account_id(&self, key: &str) -> Result<AccountId> {
        AccountId::try_from(&self.get_value(key)?)
    }

    fn get_transaction_id(&self, key: &str) -> Result<Hash> {
        Ok(Hash::try_owned_from(self.get_value(key)?)?)
    }

    fn try_get_account_id_list(&self, key: &str) -> Result<Option<Vec<AccountId>>> {
        if let Ok(array) = self.get_vec(key) {
            let account_ids = array.into_iter().map(|js_value| AccountId::try_from(&js_value)).collect::<Result<Vec<AccountId>>>()?;
            Ok(Some(account_ids))
        } else {
            Ok(None)
        }
    }
}
