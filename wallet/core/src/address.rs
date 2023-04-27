use crate::account::AccountKind;
use crate::accounts::gen0::PubkeyDerivationManagerV0;
use crate::accounts::gen0::WalletDerivationManagerV0;
use crate::accounts::gen1::PubkeyDerivationManager;
use crate::accounts::gen1::WalletDerivationManager;
use crate::accounts::PubkeyDerivationManagerTrait;
use crate::accounts::WalletDerivationManagerTrait;
use crate::storage::PubKeyData;
use crate::Result;
use futures::future::join_all;
use kaspa_addresses::{Address, Prefix};
use std::sync::{Arc, Mutex};

pub struct AddressManager {
    pub prefix: Prefix,
    pub account_kind: AccountKind,
    pub pubkey_managers: Vec<Arc<dyn PubkeyDerivationManagerTrait>>,
    pub ecdsa: bool,
    index: Arc<Mutex<u32>>,
    pub minimum_signatures: usize,
}

impl AddressManager {
    pub fn new(
        prefix: Prefix,
        account_kind: AccountKind,
        pubkey_managers: Vec<Arc<dyn PubkeyDerivationManagerTrait>>,
        ecdsa: bool,
        index: u32,
        minimum_signatures: usize,
    ) -> Result<Self> {
        let length = pubkey_managers.len();
        if length < minimum_signatures {
            return Err(format!{"The minimum amount of signatures ({}) is greater than the amount of provided public keys ({length})", minimum_signatures}.into());
        }

        for m in pubkey_managers.iter() {
            m.set_index(index)?;
        }

        Ok(Self { prefix, account_kind, pubkey_managers, ecdsa, index: Arc::new(Mutex::new(index)), minimum_signatures })
    }

    pub async fn new_address(&self) -> Result<Address> {
        self.set_index(self.index()? + 1)?;
        self.current_address().await
    }

    pub async fn current_address(&self) -> Result<Address> {
        let list = self.pubkey_managers.iter().map(|m| m.current_pubkey());

        let keys = join_all(list).await.into_iter().collect::<Result<Vec<_>>>()?;
        self.create_address(keys)
    }

    fn create_address(&self, keys: Vec<secp256k1::PublicKey>) -> Result<Address> {
        let length = keys.len();
        if length < self.minimum_signatures {
            return Err(format!{"The minimum amount of signatures ({}) is greater than the amount of provided public keys ({length})", self.minimum_signatures}.into());
        }

        if length > 1 {
            return self.create_multisig_address(keys);
        }

        if matches!(self.account_kind, AccountKind::V0) {
            PubkeyDerivationManagerV0::create_address(&keys[0], self.prefix, self.ecdsa)
        } else {
            PubkeyDerivationManager::create_address(&keys[0], self.prefix, self.ecdsa)
        }
    }

    fn create_multisig_address(&self, _keys: Vec<secp256k1::PublicKey>) -> Result<Address> {
        Err("TODO:".to_string().into())
    }

    pub fn index(&self) -> Result<u32> {
        Ok(*self.index.lock()?)
    }
    pub fn set_index(&self, index: u32) -> Result<()> {
        *self.index.lock()? = index;
        for m in self.pubkey_managers.iter() {
            m.set_index(index)?;
        }
        Ok(())
    }

    pub async fn get_range(&self, indexes: std::ops::Range<u32>) -> Result<Vec<Address>> {
        let manager_length = self.pubkey_managers.len();

        let list = self.pubkey_managers.iter().map(|m| m.get_range(indexes.clone()));

        let manager_keys = join_all(list).await.into_iter().collect::<Result<Vec<_>>>()?;

        let is_multisig = manager_length > 1;

        if !is_multisig {
            let keys = manager_keys.get(0).unwrap().clone();
            return Ok(vec![self.create_address(keys)?]);
        }

        let mut addresses = vec![];
        for key_index in indexes {
            let mut keys = vec![];
            for i in 0..manager_length {
                let k = *manager_keys.get(i).unwrap().get(key_index as usize).unwrap();
                keys.push(k);
            }
            addresses.push(self.create_address(keys)?);
        }

        Ok(addresses)
    }
}

pub struct AddressDerivationManager {
    pub receive_address_manager: Arc<AddressManager>,
    pub change_address_manager: Arc<AddressManager>,
}

impl AddressDerivationManager {
    pub async fn new(
        prefix: Prefix,
        account_kind: AccountKind,
        pub_key_data: &PubKeyData,
        ecdsa: bool,
        minimum_signatures: usize,
        receive_index: Option<u32>,
        change_index: Option<u32>,
    ) -> Result<Arc<AddressDerivationManager>> {
        let keys = &pub_key_data.keys;
        if keys.is_empty() {
            return Err("Invalid PubKeyData: no public keys".to_string().into());
        }

        let cosigner_index = pub_key_data.cosigner_index;
        let mut receive_pubkey_managers = vec![];
        let mut change_pubkey_managers = vec![];
        for xpub in keys {
            let derivator: Arc<dyn WalletDerivationManagerTrait> = match account_kind {
                AccountKind::V0 => {
                    // TODO! WalletAccountV0::from_extended_public_key is not yet implemented
                    Arc::new(WalletDerivationManagerV0::from_extended_public_key_str(xpub, cosigner_index).await?)
                }
                _ => Arc::new(WalletDerivationManager::from_extended_public_key_str(xpub, cosigner_index).await?),
            };

            receive_pubkey_managers.push(derivator.receive_pubkey_manager());
            change_pubkey_managers.push(derivator.change_pubkey_manager());
        }

        let receive_address_manager =
            AddressManager::new(prefix, account_kind, receive_pubkey_managers, ecdsa, receive_index.unwrap_or(0), minimum_signatures)?;

        let change_address_manager =
            AddressManager::new(prefix, account_kind, change_pubkey_managers, ecdsa, change_index.unwrap_or(0), minimum_signatures)?;

        let manager = Self {
            // account_kind,
            // pub_key_data: pub_key_data.clone(),
            // ecdsa,
            receive_address_manager: Arc::new(receive_address_manager),
            change_address_manager: Arc::new(change_address_manager),
        };

        Ok(manager.into())
    }

    pub fn receive_address_manager(&self) -> Arc<AddressManager> {
        self.receive_address_manager.clone()
    }

    pub fn change_address_manager(&self) -> Arc<AddressManager> {
        self.change_address_manager.clone()
    }
}
