use crate::{accounts::account::*, Result};
use async_trait::async_trait;
use futures::future::join_all;
use hmac::Mac;
use kaspa_addresses::{Address, Prefix as AddressPrefix, Version};
use kaspa_bip32::{
    types::*, AddressType, ChildNumber, DerivationPath, ExtendedKey, ExtendedKeyAttrs, ExtendedPrivateKey, ExtendedPublicKey, Prefix,
    PrivateKey, PublicKey, SecretKey, SecretKeyExt,
};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use std::{fmt::Debug, str::FromStr, sync::Arc};
use wasm_bindgen::prelude::*;
use zeroize::Zeroizing;

#[derive(Clone)]
#[wasm_bindgen]
pub struct PubkeyDerivationManagerV0 {
    /// Derived private key
    private_key: SecretKey,

    /// Extended key attributes.
    attrs: ExtendedKeyAttrs,

    #[allow(dead_code)]
    fingerprint: KeyFingerprint,

    hmac: HmacSha512,
}

impl PubkeyDerivationManagerV0 {
    pub async fn derive_pubkey(&self, index: u32) -> Result<secp256k1::PublicKey> {
        let (private_key, _) =
            WalletDerivationManagerV0::derive_private_key(&self.private_key, ChildNumber::new(index, true)?, self.hmac.clone())?;

        // let pubkey = &private_key.get_public_key().to_bytes()[1..];
        // let address = Address::new(AddressPrefix::Mainnet, Version::PubKey, pubkey);

        Ok(private_key.get_public_key())
    }

    pub async fn derive_pubkey_range(&self, indexes: std::ops::Range<u32>) -> Result<Vec<secp256k1::PublicKey>> {
        let list = indexes.map(|index| self.derive_pubkey(index)).collect::<Vec<_>>();
        let keys = join_all(list).await.into_iter().collect::<Result<Vec<_>>>()?;
        Ok(keys)
    }

    pub fn create_address(key: &secp256k1::PublicKey, prefix: AddressPrefix, _ecdsa: bool) -> Result<Address> {
        let payload = &key.to_bytes()[1..];
        let address = Address::new(prefix, Version::PubKey, payload);

        Ok(address)
    }

    #[allow(dead_code)]
    pub fn public_key(&self) -> ExtendedPublicKey<<SecretKey as PrivateKey>::PublicKey> {
        self.into()
    }

    pub fn private_key(&self) -> &SecretKey {
        &self.private_key
    }

    pub fn attrs(&self) -> &ExtendedKeyAttrs {
        &self.attrs
    }
}

impl From<&PubkeyDerivationManagerV0> for ExtendedPublicKey<<SecretKey as PrivateKey>::PublicKey> {
    fn from(inner: &PubkeyDerivationManagerV0) -> ExtendedPublicKey<<SecretKey as PrivateKey>::PublicKey> {
        ExtendedPublicKey { public_key: inner.private_key().get_public_key(), attrs: inner.attrs().clone() }
    }
}

#[async_trait]
impl PubkeyDerivationManagerTrait for PubkeyDerivationManagerV0 {
    async fn new_pubkey(&self) -> Result<secp256k1::PublicKey> {
        self.set_index(self.index()? + 1)?;
        self.current_pubkey().await
    }

    fn index(&self) -> Result<u32> {
        todo!() //Ok(*self.index.lock()?)
    }

    fn set_index(&self, _index: u32) -> Result<()> {
        todo!() //*self.index.lock()? = index;
                //Ok(())
    }

    async fn current_pubkey(&self) -> Result<secp256k1::PublicKey> {
        todo!()
        // let index = self.index()?;
        // let address = self.derive_address(index).await?;

        // Ok(address)
    }

    async fn get_range(&self, range: std::ops::Range<u32>) -> Result<Vec<secp256k1::PublicKey>> {
        self.derive_pubkey_range(range).await
    }
}

#[derive(Clone)]
#[wasm_bindgen]
pub struct WalletDerivationManagerV0 {
    /// Derived private key
    private_key: SecretKey,

    /// Extended key attributes.
    attrs: ExtendedKeyAttrs,

    /// receive address wallet
    receive_pubkey_manager: Arc<PubkeyDerivationManagerV0>,

    /// change address wallet
    change_pubkey_manager: Arc<PubkeyDerivationManagerV0>,
}

impl WalletDerivationManagerV0 {
    pub async fn derive_extened_key_from_master_key(
        xprv_key: ExtendedPrivateKey<SecretKey>,
        is_multisig: bool,
        account_index: u64,
    ) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        let attrs = xprv_key.attrs();

        let (extended_private_key, attrs) =
            Self::create_extended_key(*xprv_key.private_key(), attrs.clone(), is_multisig, account_index).await?;

        Ok((extended_private_key, attrs))
    }

    async fn create_extended_key(
        mut _private_key: SecretKey,
        mut _attrs: ExtendedKeyAttrs,
        _is_multisig: bool,
        _account_index: u64,
    ) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        // let purpose = if is_multisig { 45 } else { 44 };
        // let address_path = format!("{purpose}'/972'/{account_index}'");
        // let children = address_path.split('/');
        // for child in children {
        //     (private_key, attrs) = Self::derive_private_key(&private_key, &attrs, child.parse::<ChildNumber>()?).await?;
        // }

        // Ok((private_key, attrs))

        todo!("WIP")
    }

    pub fn build_derivate_path(
        _is_multisig: bool,
        account_index: u64,
        _cosigner_index: Option<u32>,
        address_type: Option<AddressType>,
    ) -> Result<DerivationPath> {
        // if is_multisig && cosigner_index.is_none() {
        //     return Err("cosigner_index is required for multisig path derivation".to_string().into());
        // }
        let purpose = 44; //if is_multisig { 45 } else { 44 };
        let mut path = format!("m/{purpose}'/972'/{account_index}'");
        // if let Some(cosigner_index) = cosigner_index {
        //     path = format!("{path}/{}", cosigner_index)
        // }
        if let Some(address_type) = address_type {
            path = format!("{path}/{}", address_type.index());
        }
        let path = path.parse::<DerivationPath>()?;
        Ok(path)
    }

    #[inline(always)]
    pub async fn derive_receive_pubkey(&self, index: u32) -> Result<secp256k1::PublicKey> {
        let key = self.receive_pubkey_manager.derive_pubkey(index).await?;
        Ok(key)
    }

    #[inline(always)]
    pub async fn derive_change_pubkey(&self, index: u32) -> Result<secp256k1::PublicKey> {
        let key = self.change_pubkey_manager.derive_pubkey(index).await?;
        Ok(key)
    }

    pub async fn derive_wallet(
        mut private_key: SecretKey,
        mut attrs: ExtendedKeyAttrs,
        address_type: AddressType,
    ) -> Result<PubkeyDerivationManagerV0> {
        let address_path = format!("44'/972/0'/{}'", address_type.index());
        let children = address_path.split('/');
        for child in children {
            let c = child.parse::<ChildNumber>()?;
            (private_key, attrs) = Self::derive_child(&private_key, &attrs, c).await?;
        }

        let public_key_bytes = &private_key.get_public_key().to_bytes()[1..];

        let digest = Ripemd160::digest(Sha256::digest(public_key_bytes));
        let fingerprint = digest[..4].try_into().expect("digest truncated");

        let hmac = Self::create_hmac(&private_key, &attrs, true)?;

        Ok(PubkeyDerivationManagerV0 { private_key, attrs, fingerprint, hmac })
    }

    pub async fn derive_child(
        private_key: &SecretKey,
        attrs: &ExtendedKeyAttrs,
        child_number: ChildNumber,
    ) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        let public_key_bytes = &private_key.get_public_key().to_bytes()[1..];

        let digest = Ripemd160::digest(Sha256::digest(public_key_bytes));
        let fingerprint = digest[..4].try_into().expect("digest truncated");

        let hmac = Self::create_hmac(private_key, attrs, child_number.is_hardened())?;

        let res = Self::derive_child_with_fingerprint(private_key, attrs, child_number, fingerprint, hmac).await?;

        Ok(res)
    }

    pub fn create_hmac(private_key: &SecretKey, attrs: &ExtendedKeyAttrs, hardened: bool) -> Result<HmacSha512> {
        let mut hmac = HmacSha512::new_from_slice(&attrs.chain_code).map_err(Error::Hmac)?;
        if hardened {
            hmac.update(&[0]);
            hmac.update(&private_key.to_bytes());
        } else {
            let public_key_bytes = &private_key.get_public_key().to_bytes()[1..];
            hmac.update(public_key_bytes);
        }

        Ok(hmac)
    }

    pub async fn derive_child_with_fingerprint(
        private_key: &SecretKey,
        attrs: &ExtendedKeyAttrs,
        child_number: ChildNumber,
        fingerprint: [u8; 4],
        hmac: HmacSha512,
    ) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        let (private_key, chain_code) = Self::derive_private_key(private_key, child_number, hmac)?;

        let depth = attrs.depth.checked_add(1).ok_or(Error::Depth)?;

        let attrs = ExtendedKeyAttrs { parent_fingerprint: fingerprint, child_number, chain_code, depth };

        let derived = (private_key, attrs);

        Ok(derived)
    }

    pub fn derive_private_key(
        private_key: &SecretKey,
        child_number: ChildNumber,
        mut hmac: HmacSha512,
    ) -> Result<(SecretKey, ChainCode)> {
        hmac.update(&child_number.to_bytes());

        let result = hmac.finalize().into_bytes();
        let (child_key, chain_code) = result.split_at(KEY_SIZE);

        // We should technically loop here if a `secret_key` is zero or overflows
        // the order of the underlying elliptic curve group, incrementing the
        // index, however per "Child key derivation (CKD) functions":
        // https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki#child-key-derivation-ckd-functions
        //
        // > "Note: this has probability lower than 1 in 2^127."
        //
        // ...so instead, we simply return an error if this were ever to happen,
        // as the chances of it happening are vanishingly small.
        let private_key = private_key.derive_child(child_key.try_into()?)?;

        Ok((private_key, chain_code.try_into()?))
    }

    /// Serialize the raw private key as a byte array.
    pub fn to_bytes(&self) -> PrivateKeyBytes {
        self.private_key().to_bytes()
    }

    /// Serialize this key as an [`ExtendedKey`].
    pub fn to_extended_key(&self, prefix: Prefix) -> ExtendedKey {
        // Add leading `0` byte
        let mut key_bytes = [0u8; KEY_SIZE + 1];
        key_bytes[1..].copy_from_slice(&self.to_bytes());

        ExtendedKey { prefix, attrs: self.attrs.clone(), key_bytes }
    }

    /// Serialize this key as a self-[`Zeroizing`] `String`.
    pub fn to_string(&self) -> Zeroizing<String> {
        let key = self.to_extended_key(Prefix::XPRV);

        Zeroizing::new(key.to_string())
    }

    pub fn public_key(&self) -> ExtendedPublicKey<<SecretKey as PrivateKey>::PublicKey> {
        self.into()
    }

    pub fn private_key(&self) -> &SecretKey {
        &self.private_key
    }
    pub fn attrs(&self) -> &ExtendedKeyAttrs {
        &self.attrs
    }
}

impl From<&WalletDerivationManagerV0> for ExtendedPublicKey<<SecretKey as PrivateKey>::PublicKey> {
    fn from(hd_wallet: &WalletDerivationManagerV0) -> ExtendedPublicKey<<SecretKey as PrivateKey>::PublicKey> {
        ExtendedPublicKey { public_key: hd_wallet.private_key().get_public_key(), attrs: hd_wallet.attrs().clone() }
    }
}

impl Debug for WalletDerivationManagerV0 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HDWallet")
            .field("depth", &self.attrs.depth)
            .field("child_number", &self.attrs.child_number)
            .field("chain_code", &faster_hex::hex_string(&self.attrs.chain_code))
            .field("private_key", &faster_hex::hex_string(&self.to_bytes()))
            .field("parent_fingerprint", &self.attrs.parent_fingerprint)
            .finish()
    }
}

#[async_trait]
impl WalletDerivationManagerTrait for WalletDerivationManagerV0 {
    async fn from_master_xprv(xprv: &str, _is_multisig: bool, _account_index: u64, _cosigner_index: Option<u32>) -> Result<Self> {
        let xpriv_key = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?;
        let attrs = xpriv_key.attrs();

        let receive_pubkey_manager = Self::derive_wallet(*xpriv_key.private_key(), attrs.clone(), AddressType::Receive).await?.into();

        let change_pubkey_manager = Self::derive_wallet(*xpriv_key.private_key(), attrs.clone(), AddressType::Change).await?.into();

        let wallet =
            Self { private_key: *xpriv_key.private_key(), attrs: attrs.clone(), receive_pubkey_manager, change_pubkey_manager };

        Ok(wallet)
    }

    async fn from_extended_public_key_str(_xpub: &str, _cosigner_index: Option<u32>) -> Result<Self> {
        todo!()
    }

    async fn from_extended_public_key(
        _extended_public_key: ExtendedPublicKey<secp256k1::PublicKey>,
        _cosigner_index: Option<u32>,
    ) -> Result<Self> {
        todo!()
    }

    fn receive_pubkey_manager(&self) -> Arc<dyn PubkeyDerivationManagerTrait> {
        todo!()
    }
    fn change_pubkey_manager(&self) -> Arc<dyn PubkeyDerivationManagerTrait> {
        todo!()
    }

    #[inline(always)]
    async fn derive_receive_pubkey(&self, _index: u32) -> Result<secp256k1::PublicKey> {
        // let address = self.receive_wallet.derive_pubkey(index).await?;
        // Ok(address)
        todo!()
    }

    #[inline(always)]
    async fn derive_change_pubkey(&self, _index: u32) -> Result<secp256k1::PublicKey> {
        // let address = self.change_wallet.derive_pubkey(index).await?;
        // Ok(address)
        todo!()
    }

    #[inline(always)]
    async fn receive_pubkey(&self) -> Result<secp256k1::PublicKey> {
        // let address = self.receive_wallet.new_pubkey().await?;
        // Ok(address)
        todo!()
    }

    #[inline(always)]
    async fn change_pubkey(&self) -> Result<secp256k1::PublicKey> {
        // let address = self.change_wallet.new_pubkey().await?;
        // Ok(address)
        todo!()
    }

    #[inline(always)]
    async fn new_receive_pubkey(&self) -> Result<secp256k1::PublicKey> {
        // let address = self.receive_wallet.new_pubkey().await?;
        // Ok(address)
        todo!()
    }

    #[inline(always)]
    async fn new_change_pubkey(&self) -> Result<secp256k1::PublicKey> {
        // let address = self.change_wallet.new_pubkey().await?;
        // Ok(address)
        todo!()
    }
}
