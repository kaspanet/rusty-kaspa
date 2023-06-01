use crate::accounts::gen0::PubkeyDerivationManagerV0;
use crate::accounts::gen0::WalletDerivationManagerV0;
use crate::accounts::gen1::PubkeyDerivationManager;
use crate::accounts::gen1::WalletDerivationManager;
use crate::accounts::PubkeyDerivationManagerTrait;
use crate::accounts::WalletDerivationManagerTrait;
use crate::error::Error;
use crate::runtime::AccountKind;
use crate::storage::PubKeyData;
use crate::Result;
use futures::future::join_all;
use kaspa_addresses::{Address, Prefix};
use kaspa_bip32::{AddressType, DerivationPath, ExtendedPrivateKey, ExtendedPublicKey, Language, Mnemonic, SecretKeyExt};
use kaspa_consensus_core::networktype::NetworkType;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use wasm_bindgen::prelude::*;
use workflow_wasm::tovalue::from_value;

pub struct Inner {
    pub index: u32,
    pub address_to_index_map: HashMap<Address, u32>,
}

pub struct AddressManager {
    pub prefix: Prefix,
    pub account_kind: AccountKind,
    pub pubkey_managers: Vec<Arc<dyn PubkeyDerivationManagerTrait>>,
    pub ecdsa: bool,
    pub inner: Arc<Mutex<Inner>>,
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

        let inner = Inner { index, address_to_index_map: HashMap::new() };

        Ok(Self { prefix, account_kind, pubkey_managers, ecdsa, minimum_signatures, inner: Arc::new(Mutex::new(inner)) })
    }

    pub fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub async fn new_address(&self) -> Result<Address> {
        self.set_index(self.index()? + 1)?;
        self.current_address().await
    }

    pub async fn current_address(&self) -> Result<Address> {
        let list = self.pubkey_managers.iter().map(|m| m.current_pubkey());

        let keys = join_all(list).await.into_iter().collect::<Result<Vec<_>>>()?;
        let address = self.create_address(keys)?;

        self.update_address_to_index_map(self.index()?, &[address.clone()])?;

        Ok(address)
    }

    fn create_address(&self, keys: Vec<secp256k1::PublicKey>) -> Result<Address> {
        create_address(self.minimum_signatures, keys, self.prefix, self.ecdsa, Some(self.account_kind))
    }

    pub fn index(&self) -> Result<u32> {
        Ok(self.inner().index)
    }
    pub fn set_index(&self, index: u32) -> Result<()> {
        self.inner().index = index;
        for m in self.pubkey_managers.iter() {
            m.set_index(index)?;
        }
        Ok(())
    }

    pub async fn get_range(&self, indexes: std::ops::Range<u32>) -> Result<Vec<Address>> {
        self.get_range_with_args(indexes, true).await
    }

    pub async fn get_range_with_args(&self, indexes: std::ops::Range<u32>, update_indexes: bool) -> Result<Vec<Address>> {
        let manager_length = self.pubkey_managers.len();

        let list = self.pubkey_managers.iter().map(|m| m.get_range(indexes.clone()));

        let manager_keys = join_all(list).await.into_iter().collect::<Result<Vec<_>>>()?;

        let is_multisig = manager_length > 1;

        if !is_multisig {
            let keys = manager_keys.get(0).unwrap().clone();
            let mut addresses = vec![];
            for key in keys {
                addresses.push(self.create_address(vec![key])?);
            }
            if update_indexes {
                self.update_address_to_index_map(indexes.start, &addresses)?;
            }
            return Ok(addresses);
        }

        let mut addresses = vec![];
        for key_index in indexes.clone() {
            let mut keys = vec![];
            for i in 0..manager_length {
                let k = *manager_keys.get(i).unwrap().get(key_index as usize).unwrap();
                keys.push(k);
            }
            addresses.push(self.create_address(keys)?);
        }

        if update_indexes {
            self.update_address_to_index_map(indexes.start, &addresses)?;
        }
        Ok(addresses)
    }

    fn update_address_to_index_map(&self, offset: u32, addresses: &[Address]) -> Result<()> {
        let address_to_index_map = &mut self.inner().address_to_index_map;
        for (index, address) in addresses.iter().enumerate() {
            address_to_index_map.insert(address.clone(), offset + index as u32);
        }

        Ok(())
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

    pub fn addresses_indexes(&self, addresses: &Vec<Address>) -> Result<(Vec<u32>, Vec<u32>)> {
        let mut receive_indexes = vec![];
        let mut change_indexes = vec![];
        let receive_map = &self.receive_address_manager.inner().address_to_index_map;
        let change_map = &self.change_address_manager.inner().address_to_index_map;

        for address in addresses {
            if let Some(index) = receive_map.get(address) {
                receive_indexes.push(*index);
            } else if let Some(index) = change_map.get(address) {
                change_indexes.push(*index);
            } else {
                return Err(Error::Custom(format!("Address ({address}) index not found.")));
            }
        }

        Ok((receive_indexes, change_indexes))
    }

    pub fn receive_indexes_by_addresses(&self, addresses: &Vec<Address>) -> Result<Vec<u32>> {
        self.indexes_by_addresses(addresses, &self.receive_address_manager)
    }
    pub fn change_indexes_by_addresses(&self, addresses: &Vec<Address>) -> Result<Vec<u32>> {
        self.indexes_by_addresses(addresses, &self.change_address_manager)
    }

    pub fn indexes_by_addresses(&self, addresses: &Vec<Address>, manager: &Arc<AddressManager>) -> Result<Vec<u32>> {
        let map = &manager.inner().address_to_index_map;
        let mut indexes = vec![];
        for address in addresses {
            let index = map.get(address).ok_or(Error::Custom(format!("Address ({address}) index not found.")))?;
            indexes.push(*index);
        }

        Ok(indexes)
    }

    pub fn receive_indexes(&self) -> Result<Vec<u32>> {
        self.indexes(&self.receive_address_manager)
    }
    pub fn change_indexes(&self) -> Result<Vec<u32>> {
        self.indexes(&self.change_address_manager)
    }
    pub fn indexes(&self, manager: &Arc<AddressManager>) -> Result<Vec<u32>> {
        let map = &manager.inner().address_to_index_map;
        let mut indexes = vec![];
        for (_, index) in map.iter() {
            indexes.push(*index);
        }
        Ok(indexes)
    }
}

pub fn create_multisig_address(_keys: Vec<secp256k1::PublicKey>) -> Result<Address> {
    Err("TODO: multisig_address".to_string().into())
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Array, typescript_type="Array")]
    pub type PublicKeys;
}

#[wasm_bindgen(js_name=createAddress)]
pub fn create_address_js(
    key: &str,
    network_type: NetworkType,
    ecdsa: Option<bool>,
    account_kind: Option<AccountKind>,
) -> Result<Address> {
    let key: secp256k1::PublicKey = from_value(key.into())?;
    create_address(1, vec![key], network_type.into(), ecdsa.unwrap_or(false), account_kind)
}

#[wasm_bindgen(js_name=createMultisigAddress)]
pub fn create_multisig_address_js(
    minimum_signatures: usize,
    keys: PublicKeys,
    network_type: NetworkType,
    ecdsa: Option<bool>,
    account_kind: Option<AccountKind>,
) -> Result<Address> {
    let keys: Vec<secp256k1::PublicKey> = from_value(keys.into())?;
    create_address(minimum_signatures, keys, network_type.into(), ecdsa.unwrap_or(false), account_kind)
}

pub fn create_address(
    minimum_signatures: usize,
    keys: Vec<secp256k1::PublicKey>,
    prefix: Prefix,
    ecdsa: bool,
    account_kind: Option<AccountKind>,
) -> Result<Address> {
    let length = keys.len();
    if length < minimum_signatures {
        return Err(format!{"The minimum amount of signatures ({}) is greater than the amount of provided public keys ({length})", minimum_signatures}.into());
    }

    if length > 1 {
        return create_multisig_address(keys);
    }

    if matches!(account_kind, Some(AccountKind::V0)) {
        PubkeyDerivationManagerV0::create_address(&keys[0], prefix, ecdsa)
    } else {
        PubkeyDerivationManager::create_address(&keys[0], prefix, ecdsa)
    }
}

pub async fn create_xpub_from_mnemonic(
    seed_words: &str,
    account_kind: AccountKind,
    account_index: u64,
) -> Result<ExtendedPublicKey<secp256k1::PublicKey>> {
    let mnemonic = Mnemonic::new(seed_words, Language::English)?;
    let seed = mnemonic.to_seed("");
    let xkey = ExtendedPrivateKey::<secp256k1::SecretKey>::new(seed)?;

    let (secret_key, attrs) = match account_kind {
        AccountKind::V0 => WalletDerivationManagerV0::derive_extened_key_from_master_key(xkey, true, account_index).await?,
        AccountKind::MultiSig => WalletDerivationManager::derive_extened_key_from_master_key(xkey, true, account_index).await?,
        _ => WalletDerivationManager::derive_extened_key_from_master_key(xkey, false, account_index).await?,
    };

    let xkey = ExtendedPublicKey { public_key: secret_key.get_public_key(), attrs };

    Ok(xkey)
}

pub fn build_derivate_path(
    account_kind: AccountKind,
    account_index: u64,
    cosigner_index: u32,
    address_type: AddressType,
) -> Result<DerivationPath> {
    match account_kind {
        AccountKind::V0 => WalletDerivationManagerV0::build_derivate_path(false, account_index, None, Some(address_type)),
        AccountKind::Bip32 => WalletDerivationManager::build_derivate_path(false, account_index, None, Some(address_type)),
        AccountKind::MultiSig => {
            WalletDerivationManager::build_derivate_path(true, account_index, Some(cosigner_index), Some(address_type))
        }
    }
}

pub fn build_derivate_paths(
    account_kind: AccountKind,
    account_index: u64,
    cosigner_index: u32,
) -> Result<(DerivationPath, DerivationPath)> {
    let receive_path = build_derivate_path(account_kind, account_index, cosigner_index, AddressType::Receive)?;
    let change_path = build_derivate_path(account_kind, account_index, cosigner_index, AddressType::Change)?;
    Ok((receive_path, change_path))
}
