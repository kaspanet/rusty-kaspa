//!
//!  Module handling bip32 address derivation (bip32+bip44 and legacy accounts)
//!

use kaspa_wallet_keys::derivation::gen0::{PubkeyDerivationManagerV0, WalletDerivationManagerV0};
use kaspa_wallet_keys::derivation::gen1::{PubkeyDerivationManager, WalletDerivationManager};

pub use kaspa_wallet_keys::derivation::traits::*;
use kaspa_wallet_keys::publickey::{PublicKey, PublicKeyArrayT, PublicKeyT};
pub use kaspa_wallet_keys::types::*;

use crate::account::create_private_keys;
use crate::account::AccountKind;
use crate::error::Error;
use crate::imports::*;
use crate::result::Result;
use kaspa_bip32::{AddressType, DerivationPath, ExtendedPrivateKey, ExtendedPublicKey, Language, Mnemonic, SecretKeyExt};
use kaspa_consensus_core::network::NetworkType;
use kaspa_txscript::{
    extract_script_pub_key_address, multisig_redeem_script, multisig_redeem_script_ecdsa, pay_to_script_hash_script,
};

#[derive(Default, Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AddressDerivationMeta([u32; 2]);

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

impl std::fmt::Display for AddressDerivationMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}, {}]", self.receive(), self.change())
    }
}

pub struct Inner {
    pub index: u32,
    pub address_to_index_map: HashMap<Address, u32>,
}

pub struct AddressManager {
    pub wallet: Arc<Wallet>,
    pub account_kind: AccountKind,
    pub pubkey_managers: Vec<Arc<dyn PubkeyDerivationManagerTrait>>,
    pub ecdsa: bool,
    pub inner: Arc<Mutex<Inner>>,
    pub minimum_signatures: usize,
}

impl AddressManager {
    pub fn new(
        wallet: Arc<Wallet>,
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
        let keys = list.into_iter().collect::<kaspa_wallet_keys::result::Result<Vec<_>>>()?;
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

        let manager_keys = list.into_iter().collect::<kaspa_wallet_keys::result::Result<Vec<_>>>()?;

        let is_multisig = manager_length > 1;

        if !is_multisig {
            let keys = manager_keys.first().unwrap().clone();
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
    pub derivators: Vec<Arc<dyn WalletDerivationManagerTrait>>,
    #[allow(dead_code)]
    wallet: Arc<Wallet>,
    pub receive_address_manager: Arc<AddressManager>,
    pub change_address_manager: Arc<AddressManager>,
}

impl AddressDerivationManager {
    pub async fn new(
        wallet: &Arc<Wallet>,
        account_kind: AccountKind,
        keys: &ExtendedPublicKeys,
        ecdsa: bool,
        account_index: u64,
        cosigner_index: Option<u32>,
        minimum_signatures: u16,
        address_derivation_indexes: AddressDerivationMeta,
    ) -> Result<Arc<AddressDerivationManager>> {
        if keys.is_empty() {
            return Err("Invalid keys: keys are required for address derivation".to_string().into());
        }

        let mut receive_pubkey_managers = vec![];
        let mut change_pubkey_managers = vec![];
        let mut derivators = vec![];
        for xpub in keys.iter() {
            let derivator: Arc<dyn WalletDerivationManagerTrait> = match account_kind.as_ref() {
                LEGACY_ACCOUNT_KIND => Arc::new(WalletDerivationManagerV0::from_extended_public_key(xpub.clone(), cosigner_index)?),
                MULTISIG_ACCOUNT_KIND => {
                    let cosigner_index = cosigner_index.ok_or(Error::InvalidAccountKind)?;
                    Arc::new(WalletDerivationManager::from_extended_public_key(xpub.clone(), Some(cosigner_index))?)
                }
                _ => Arc::new(WalletDerivationManager::from_extended_public_key(xpub.clone(), cosigner_index)?),
            };

            receive_pubkey_managers.push(derivator.receive_pubkey_manager());
            change_pubkey_managers.push(derivator.change_pubkey_manager());
            derivators.push(derivator);
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
            derivators,
            wallet: wallet.clone(),
            receive_address_manager: Arc::new(receive_address_manager),
            change_address_manager: Arc::new(change_address_manager),
        };

        Ok(manager.into())
    }

    pub fn create_legacy_pubkey_managers(
        wallet: &Arc<Wallet>,
        account_index: u64,
        address_derivation_indexes: AddressDerivationMeta,
    ) -> Result<Arc<AddressDerivationManager>> {
        let mut receive_pubkey_managers = vec![];
        let mut change_pubkey_managers = vec![];
        let derivator: Arc<dyn WalletDerivationManagerTrait> =
            Arc::new(WalletDerivationManagerV0::create_uninitialized(account_index, None, None)?);
        receive_pubkey_managers.push(derivator.receive_pubkey_manager());
        change_pubkey_managers.push(derivator.change_pubkey_manager());

        let account_kind = AccountKind::from(LEGACY_ACCOUNT_KIND);

        let receive_address_manager = AddressManager::new(
            wallet.clone(),
            account_kind,
            receive_pubkey_managers,
            false,
            address_derivation_indexes.receive(),
            1,
        )?;

        let change_address_manager =
            AddressManager::new(wallet.clone(), account_kind, change_pubkey_managers, false, address_derivation_indexes.change(), 1)?;

        let manager = Self {
            account_kind,
            account_index,
            cosigner_index: None,
            derivators: vec![derivator],
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
        xkey: &ExtendedPrivateKey<secp256k1::SecretKey>,
    ) -> Result<Vec<(Address, secp256k1::SecretKey)>> {
        self.get_range_with_keys_impl(false, indexes, update_indexes, xkey).await
    }

    pub async fn get_change_range_with_keys(
        &self,
        indexes: std::ops::Range<u32>,
        update_indexes: bool,
        xkey: &ExtendedPrivateKey<secp256k1::SecretKey>,
    ) -> Result<Vec<(Address, secp256k1::SecretKey)>> {
        self.get_range_with_keys_impl(true, indexes, update_indexes, xkey).await
    }

    async fn get_range_with_keys_impl(
        &self,
        change_address: bool,
        indexes: std::ops::Range<u32>,
        update_indexes: bool,
        xkey: &ExtendedPrivateKey<secp256k1::SecretKey>,
    ) -> Result<Vec<(Address, secp256k1::SecretKey)>> {
        let start = indexes.start;
        let addresses = if change_address {
            self.change_address_manager.get_range_with_args(indexes, update_indexes)?
        } else {
            self.receive_address_manager.get_range_with_args(indexes, update_indexes)?
        };

        let addresses = addresses.iter().enumerate().map(|(index, a)| (a, start + index as u32)).collect::<Vec<(&Address, u32)>>();

        let (receive, change) = if change_address { (vec![], addresses) } else { (addresses, vec![]) };

        let private_keys =
            create_private_keys(&self.account_kind, self.cosigner_index.unwrap_or(0), self.account_index, xkey, &receive, &change)?;

        let mut result = vec![];
        for (address, private_key) in private_keys {
            result.push((address.clone(), private_key));
        }

        Ok(result)
    }

    #[allow(clippy::type_complexity)]
    pub fn get_addresses_indexes<'l>(&self, addresses: &[&'l Address]) -> Result<(Vec<(&'l Address, u32)>, Vec<(&'l Address, u32)>)> {
        let mut receive_indexes = vec![];
        let mut change_indexes = vec![];
        let receive_map = &self.receive_address_manager.inner().address_to_index_map;
        let change_map = &self.change_address_manager.inner().address_to_index_map;

        for address in addresses {
            if let Some(index) = receive_map.get(*address) {
                receive_indexes.push((*address, *index));
            } else if let Some(index) = change_map.get(*address) {
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

#[async_trait]
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

    async fn get_range_with_keys(
        &self,
        change_address: bool,
        indexes: std::ops::Range<u32>,
        update_indexes: bool,
        xkey: &ExtendedPrivateKey<secp256k1::SecretKey>,
    ) -> Result<Vec<(Address, secp256k1::SecretKey)>> {
        Ok(self.get_range_with_keys_impl(change_address, indexes, update_indexes, xkey).await?)
    }
}

#[async_trait]
pub trait AddressDerivationManagerTrait: AnySync + Send + Sync + 'static {
    fn receive_address_manager(&self) -> Arc<AddressManager>;
    fn change_address_manager(&self) -> Arc<AddressManager>;
    #[allow(clippy::type_complexity)]
    fn addresses_indexes<'l>(&self, addresses: &[&'l Address]) -> Result<(Vec<(&'l Address, u32)>, Vec<(&'l Address, u32)>)>;
    async fn get_range_with_keys(
        &self,
        change_address: bool,
        indexes: std::ops::Range<u32>,
        update_indexes: bool,
        xkey: &ExtendedPrivateKey<secp256k1::SecretKey>,
    ) -> Result<Vec<(Address, secp256k1::SecretKey)>>;
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

/// @category Wallet SDK
#[wasm_bindgen(js_name=createAddress)]
pub fn create_address_js(
    key: PublicKeyT,
    network_type: NetworkType,
    ecdsa: Option<bool>,
    account_kind: Option<AccountKind>,
) -> Result<Address> {
    let public_key = PublicKey::try_cast_from(key)?;
    create_address(1, vec![public_key.as_ref().try_into()?], network_type.into(), ecdsa.unwrap_or(false), account_kind)
}

/// @category Wallet SDK
#[wasm_bindgen(js_name=createMultisigAddress)]
pub fn create_multisig_address_js(
    minimum_signatures: usize,
    keys: PublicKeyArrayT,
    network_type: NetworkType,
    ecdsa: Option<bool>,
    account_kind: Option<AccountKind>,
) -> Result<Address> {
    create_address(minimum_signatures, keys.try_into()?, network_type.into(), ecdsa.unwrap_or(false), account_kind)
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

    if account_kind.map(|kind| kind == LEGACY_ACCOUNT_KIND).unwrap_or(false) {
        Ok(PubkeyDerivationManagerV0::create_address(&keys[0], prefix, ecdsa)?)
    } else {
        Ok(PubkeyDerivationManager::create_address(&keys[0], prefix, ecdsa)?)
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

    let (secret_key, attrs) = match account_kind.as_ref() {
        LEGACY_ACCOUNT_KIND => WalletDerivationManagerV0::derive_extended_key_from_master_key(xkey, false, account_index)?,
        MULTISIG_ACCOUNT_KIND => WalletDerivationManager::derive_extended_key_from_master_key(xkey, true, account_index)?,
        _ => WalletDerivationManager::derive_extended_key_from_master_key(xkey, false, account_index)?,
    };

    let xkey = ExtendedPublicKey { public_key: secret_key.get_public_key(), attrs };

    Ok(xkey)
}

pub async fn create_xpub_from_xprv(
    xprv: ExtendedPrivateKey<secp256k1::SecretKey>,
    account_kind: AccountKind,
    account_index: u64,
) -> Result<ExtendedPublicKey<secp256k1::PublicKey>> {
    let (secret_key, attrs) = match account_kind.as_ref() {
        LEGACY_ACCOUNT_KIND => WalletDerivationManagerV0::derive_extended_key_from_master_key(xprv, false, account_index)?,
        MULTISIG_ACCOUNT_KIND => WalletDerivationManager::derive_extended_key_from_master_key(xprv, true, account_index)?,
        BIP32_ACCOUNT_KIND => WalletDerivationManager::derive_extended_key_from_master_key(xprv, false, account_index)?,
        _ => panic!("create_xpub_from_xprv not supported for account kind: {:?}", account_kind),
    };

    let xkey = ExtendedPublicKey { public_key: secret_key.get_public_key(), attrs };

    Ok(xkey)
}

pub fn build_derivate_path(
    account_kind: &AccountKind,
    account_index: u64,
    cosigner_index: u32,
    address_type: AddressType,
) -> Result<DerivationPath> {
    match account_kind.as_ref() {
        LEGACY_ACCOUNT_KIND => Ok(WalletDerivationManagerV0::build_derivate_path(account_index, Some(address_type))?),
        BIP32_ACCOUNT_KIND => Ok(WalletDerivationManager::build_derivate_path(false, account_index, None, Some(address_type))?),
        MULTISIG_ACCOUNT_KIND => {
            Ok(WalletDerivationManager::build_derivate_path(true, account_index, Some(cosigner_index), Some(address_type))?)
        }
        _ => {
            panic!("build derivate path not supported for account kind: {:?}", account_kind);
        }
    }
}

pub fn build_derivate_paths(
    account_kind: &AccountKind,
    account_index: u64,
    cosigner_index: u32,
) -> Result<(DerivationPath, DerivationPath)> {
    let receive_path = build_derivate_path(account_kind, account_index, cosigner_index, AddressType::Receive)?;
    let change_path = build_derivate_path(account_kind, account_index, cosigner_index, AddressType::Change)?;
    Ok((receive_path, change_path))
}
