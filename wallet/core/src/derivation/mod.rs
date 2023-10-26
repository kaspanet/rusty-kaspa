pub mod gen0;
pub mod gen1;
pub mod traits;

pub use traits::*;

use crate::derivation::gen0::{PubkeyDerivationManagerV0, WalletDerivationManagerV0};
use crate::derivation::gen1::{PubkeyDerivationManager, WalletDerivationManager};
use crate::error::Error;
use crate::imports::*;
use crate::runtime;
use crate::runtime::account::create_private_keys;
use crate::runtime::AccountKind;
use crate::secret::Secret;
use crate::storage::PrvKeyDataId;
use crate::Result;
use kaspa_bip32::{AddressType, DerivationPath, ExtendedPrivateKey, ExtendedPublicKey, Language, Mnemonic, SecretKeyExt};
use kaspa_consensus_core::network::NetworkType;
use kaspa_txscript::{
    extract_script_pub_key_address, multisig_redeem_script, multisig_redeem_script_ecdsa, pay_to_script_hash_script,
};
use kaspa_utils::hex::ToHex;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use wasm_bindgen::prelude::*;
use workflow_wasm::serde::from_value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddressDerivationMeta([u32; 2]);

impl Default for AddressDerivationMeta {
    fn default() -> Self {
        Self([1, 1])
    }
}

impl AddressDerivationMeta {
    pub fn new(receive: u32, change: u32) -> Self {
        Self([receive, change])
    }

    pub fn receive(&self) -> u32 {
        self.0[0]
    }

    pub fn change(&self) -> u32 {
        self.0[1]
    }
}

pub struct Inner {
    pub index: u32,
    pub address_to_index_map: HashMap<Address, u32>,
}

pub struct AddressManager {
    // pub prefix: Prefix,
    pub wallet: Arc<runtime::Wallet>,
    pub account_kind: AccountKind,
    pub pubkey_managers: Vec<Arc<dyn PubkeyDerivationManagerTrait>>,
    pub ecdsa: bool,
    pub inner: Arc<Mutex<Inner>>,
    pub minimum_signatures: usize,
}

impl AddressManager {
    pub fn new(
        // prefix: Prefix,
        wallet: Arc<runtime::Wallet>,
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

        Ok(Self { wallet, account_kind, pubkey_managers, ecdsa, minimum_signatures, inner: Arc::new(Mutex::new(inner)) })
    }

    pub fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub fn new_address(&self) -> Result<Address> {
        self.set_index(self.index() + 1)?;
        self.current_address()
    }

    pub fn current_address(&self) -> Result<Address> {
        let list = self.pubkey_managers.iter().map(|m| m.current_pubkey());

        // let keys = join_all(list).await.into_iter().collect::<Result<Vec<_>>>()?;
        let keys = list.into_iter().collect::<Result<Vec<_>>>()?;
        let address = self.create_address(keys)?;

        self.update_address_to_index_map(self.index(), &[address.clone()])?;

        Ok(address)
    }

    fn create_address(&self, keys: Vec<secp256k1::PublicKey>) -> Result<Address> {
        let address_prefix = self.wallet.address_prefix()?;
        create_address(self.minimum_signatures, keys, address_prefix, self.ecdsa, Some(self.account_kind))
    }

    pub fn index(&self) -> u32 {
        self.inner().index
    }
    pub fn set_index(&self, index: u32) -> Result<()> {
        self.inner().index = index;
        for m in self.pubkey_managers.iter() {
            m.set_index(index)?;
        }
        Ok(())
    }

    pub fn get_range(&self, indexes: std::ops::Range<u32>) -> Result<Vec<Address>> {
        self.get_range_with_args(indexes, true)
    }

    pub fn get_range_with_args(&self, indexes: std::ops::Range<u32>, update_indexes: bool) -> Result<Vec<Address>> {
        let manager_length = self.pubkey_managers.len();

        let list = self.pubkey_managers.iter().map(|m| m.get_range(indexes.clone()));

        // let manager_keys = join_all(list).await.into_iter().collect::<Result<Vec<_>>>()?;
        let manager_keys = list.into_iter().collect::<Result<Vec<_>>>()?;

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
                let Some(k) = manager_keys.get(i).unwrap().get(key_index as usize) else { continue };
                keys.push(*k);
            }
            if keys.is_empty() {
                continue;
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
    pub account_kind: AccountKind,
    pub account_index: u64,
    pub cosigner_index: Option<u32>,
    wallet: Arc<runtime::Wallet>,
    pub receive_address_manager: Arc<AddressManager>,
    pub change_address_manager: Arc<AddressManager>,
}

impl AddressDerivationManager {
    pub async fn new(
        wallet: &Arc<runtime::Wallet>,
        // prefix: Prefix,
        account_kind: AccountKind,
        keys: &Vec<String>,
        // pub_key_data: &PubKeyData,
        ecdsa: bool,
        account_index: u64,
        cosigner_index: Option<u32>,
        minimum_signatures: u16,
        address_derivation_indexes: AddressDerivationMeta,
        // receive_index: Option<u32>,
        // change_index: Option<u32>,
    ) -> Result<Arc<AddressDerivationManager>> {
        // let keys = &pub_key_data.keys;
        if keys.is_empty() {
            return Err("Invalid keys: keys are required for address derivation".to_string().into());
        }

        // let cosigner_index = pub_key_data.cosigner_index;
        let mut receive_pubkey_managers = vec![];
        let mut change_pubkey_managers = vec![];
        for xpub in keys {
            let derivator: Arc<dyn WalletDerivationManagerTrait> = match account_kind {
                AccountKind::Legacy => {
                    // TODO! WalletAccountV0::from_extended_public_key is not yet implemented
                    Arc::new(gen0::WalletDerivationManagerV0::from_extended_public_key_str(xpub, cosigner_index)?)
                }
                AccountKind::MultiSig => {
                    let cosigner_index = cosigner_index.ok_or(Error::InvalidAccountKind)?;
                    Arc::new(gen1::WalletDerivationManager::from_extended_public_key_str(xpub, Some(cosigner_index))?)
                }
                _ => Arc::new(gen1::WalletDerivationManager::from_extended_public_key_str(xpub, cosigner_index)?),
            };

            receive_pubkey_managers.push(derivator.receive_pubkey_manager());
            change_pubkey_managers.push(derivator.change_pubkey_manager());
        }

        let receive_address_manager = AddressManager::new(
            wallet.clone(),
            account_kind,
            receive_pubkey_managers,
            ecdsa,
            address_derivation_indexes.receive(),
            minimum_signatures as usize, //.unwrap_or(1) as usize,
        )?;

        let change_address_manager = AddressManager::new(
            wallet.clone(),
            account_kind,
            change_pubkey_managers,
            ecdsa,
            address_derivation_indexes.change(),
            minimum_signatures as usize, //.unwrap_or(1) as usize,
        )?;

        let manager = Self {
            account_kind,
            account_index,
            cosigner_index,
            wallet: wallet.clone(),
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

    pub async fn get_receive_range_with_keys(
        &self,
        indexes: std::ops::Range<u32>,
        update_indexes: bool,
        wallet_secret: Secret,
        payment_secret: &Option<Secret>,
        id: &PrvKeyDataId,
    ) -> Result<Vec<(Address, secp256k1::SecretKey)>> {
        self.get_range_with_keys(false, indexes, update_indexes, wallet_secret, payment_secret, id).await
    }

    pub async fn get_change_range_with_keys(
        &self,
        indexes: std::ops::Range<u32>,
        update_indexes: bool,
        wallet_secret: Secret,
        payment_secret: &Option<Secret>,
        id: &PrvKeyDataId,
    ) -> Result<Vec<(Address, secp256k1::SecretKey)>> {
        self.get_range_with_keys(true, indexes, update_indexes, wallet_secret, payment_secret, id).await
    }

    async fn get_range_with_keys(
        &self,
        change_address: bool,
        indexes: std::ops::Range<u32>,
        update_indexes: bool,
        wallet_secret: Secret,
        payment_secret: &Option<Secret>,
        id: &PrvKeyDataId,
    ) -> Result<Vec<(Address, secp256k1::SecretKey)>> {
        let addresses = if change_address {
            self.change_address_manager.get_range_with_args(indexes, update_indexes)?
        } else {
            self.receive_address_manager.get_range_with_args(indexes, update_indexes)?
        };

        let addresses_list = &addresses.iter().collect::<Vec<&Address>>()[..];
        let (receive, change) = self.addresses_indexes(addresses_list)?;
        let keydata = match self.wallet.get_prv_key_data(wallet_secret, id).await? {
            Some(keydata) => keydata,
            None => return Err(Error::KeyId(id.to_hex())),
        };

        let private_keys = create_private_keys(
            self.account_kind,
            self.cosigner_index.unwrap_or(0),
            self.account_index,
            &keydata,
            payment_secret,
            &receive,
            &change,
        )?;

        let mut result = vec![];
        for (address, private_key) in private_keys {
            result.push((address.clone(), private_key));
        }

        Ok(result)
    }

    // pub fn addresses_indexes(&self, addresses: &Vec<Address>) -> Result<(Vec<u32>, Vec<u32>)> {
    //     let mut receive_indexes = vec![];
    //     let mut change_indexes = vec![];
    //     let receive_map = &self.receive_address_manager.inner().address_to_index_map;
    //     let change_map = &self.change_address_manager.inner().address_to_index_map;

    //     for address in addresses {
    //         if let Some(index) = receive_map.get(address) {
    //             receive_indexes.push(*index);
    //         } else if let Some(index) = change_map.get(address) {
    //             change_indexes.push(*index);
    //         } else {
    //             return Err(Error::Custom(format!("Address ({address}) index not found.")));
    //         }
    //     }

    //     Ok((receive_indexes, change_indexes))
    // }

    #[allow(clippy::type_complexity)]
    pub fn get_addresses_indexes<'l>(&self, addresses: &[&'l Address]) -> Result<(Vec<(&'l Address, u32)>, Vec<(&'l Address, u32)>)> {
        let mut receive_indexes = vec![];
        let mut change_indexes = vec![];
        let receive_map = &self.receive_address_manager.inner().address_to_index_map;
        let change_map = &self.change_address_manager.inner().address_to_index_map;

        for address in addresses {
            if let Some(index) = receive_map.get(address) {
                receive_indexes.push((*address, *index));
            } else if let Some(index) = change_map.get(address) {
                change_indexes.push((*address, *index));
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

    pub fn address_derivation_meta(&self) -> AddressDerivationMeta {
        AddressDerivationMeta::new(self.receive_address_manager.index(), self.change_address_manager.index())
    }
}

impl AddressDerivationManagerTrait for AddressDerivationManager {
    fn receive_address_manager(&self) -> Arc<AddressManager> {
        self.receive_address_manager.clone()
    }

    fn change_address_manager(&self) -> Arc<AddressManager> {
        self.change_address_manager.clone()
    }

    #[allow(clippy::type_complexity)]
    fn addresses_indexes<'l>(&self, addresses: &[&'l Address]) -> Result<(Vec<(&'l Address, u32)>, Vec<(&'l Address, u32)>)> {
        self.get_addresses_indexes(addresses)
    }
}

pub trait AddressDerivationManagerTrait: AnySync + Send + Sync + 'static {
    fn receive_address_manager(&self) -> Arc<AddressManager>;
    fn change_address_manager(&self) -> Arc<AddressManager>;
    #[allow(clippy::type_complexity)]
    fn addresses_indexes<'l>(&self, addresses: &[&'l Address]) -> Result<(Vec<(&'l Address, u32)>, Vec<(&'l Address, u32)>)>;
}

pub fn create_multisig_address(
    minimum_signatures: usize,
    keys: Vec<secp256k1::PublicKey>,
    prefix: Prefix,
    ecdsa: bool,
) -> Result<Address> {
    let script = if !ecdsa {
        multisig_redeem_script(keys.iter().map(|pk| pk.x_only_public_key().0.serialize()), minimum_signatures)
    } else {
        multisig_redeem_script_ecdsa(keys.iter().map(|pk| pk.serialize()), minimum_signatures)
    }?;
    let script_pub_key = pay_to_script_hash_script(&script);
    let address = extract_script_pub_key_address(&script_pub_key, prefix)?;
    Ok(address)
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
        return create_multisig_address(minimum_signatures, keys, prefix, ecdsa);
    }

    if matches!(account_kind, Some(AccountKind::Legacy)) {
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
        AccountKind::Legacy => WalletDerivationManagerV0::derive_extened_key_from_master_key(xkey, true, account_index)?,
        AccountKind::MultiSig => WalletDerivationManager::derive_extened_key_from_master_key(xkey, true, account_index)?,
        _ => gen1::WalletDerivationManager::derive_extened_key_from_master_key(xkey, false, account_index)?,
    };

    let xkey = ExtendedPublicKey { public_key: secret_key.get_public_key(), attrs };

    Ok(xkey)
}

pub async fn create_xpub_from_xprv(
    xprv: ExtendedPrivateKey<secp256k1::SecretKey>,
    account_kind: AccountKind,
    account_index: u64,
) -> Result<ExtendedPublicKey<secp256k1::PublicKey>> {
    let (secret_key, attrs) = match account_kind {
        AccountKind::Legacy => WalletDerivationManagerV0::derive_extened_key_from_master_key(xprv, true, account_index)?,
        AccountKind::MultiSig => WalletDerivationManager::derive_extened_key_from_master_key(xprv, true, account_index)?,
        AccountKind::Bip32 => WalletDerivationManager::derive_extened_key_from_master_key(xprv, false, account_index)?,
        _ => panic!("create_xpub_from_xprv not supported for account kind: {:?}", account_kind),
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
        AccountKind::Legacy => WalletDerivationManagerV0::build_derivate_path(account_index, None),
        AccountKind::Bip32 => WalletDerivationManager::build_derivate_path(false, account_index, None, Some(address_type)),
        AccountKind::MultiSig => {
            WalletDerivationManager::build_derivate_path(true, account_index, Some(cosigner_index), Some(address_type))
        }
        _ => {
            panic!("build derivate path not supported for account kind: {:?}", account_kind);
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
