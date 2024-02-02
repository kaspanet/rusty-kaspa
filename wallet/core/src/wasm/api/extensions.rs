use crate::imports::*;
use js_sys::Object;

pub trait WalletApiObjectExtension {
    fn get_secret(&self, key: &str) -> Result<Secret>;
    fn get_network_id(&self, key: &str) -> Result<NetworkId>;
    fn get_prv_key_data_id(&self, key: &str) -> Result<PrvKeyDataId>;
    fn get_account_id(&self, key: &str) -> Result<AccountId>;
    fn try_get_account_id_list(&self, key: &str) -> Result<Option<Vec<AccountId>>>;
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

    fn get_network_id(&self, key: &str) -> Result<NetworkId> {
        let value = self.get_value(key)?;
        Ok(NetworkId::try_from(value)?)
    }

    fn get_prv_key_data_id(&self, key: &str) -> Result<PrvKeyDataId> {
        PrvKeyDataId::try_from(&self.get_value(key)?)
    }

    fn get_account_id(&self, key: &str) -> Result<AccountId> {
        AccountId::try_from(&self.get_value(key)?)
    }

    fn try_get_account_id_list(&self, key: &str) -> Result<Option<Vec<AccountId>>> {
        // TODO - check for undefined
        let array = self.get_vec(key)?;
        let account_ids = array.into_iter().map(|js_value| AccountId::try_from(&js_value)).collect::<Result<Vec<AccountId>>>()?;
        Ok(Some(account_ids))
    }
}
