use addresses::{Address, Prefix as AddressPrefix};
use hmac::Mac;
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use std::{
    fmt::Debug,
    str::FromStr,
    sync::{Arc, Mutex}
};
use zeroize::Zeroizing;

use kaspa_bip32::{
    types::*, AddressType, ChildNumber, ExtendedKey, ExtendedKeyAttrs, ExtendedPrivateKey,
    ExtendedPublicKey, Prefix, PrivateKey, PublicKey, SecretKey, SecretKeyExt,
};

fn get_fingerprint<K>(private_key: &K) -> KeyFingerprint
where
    K: PrivateKey,
{
    let public_key_bytes = private_key.public_key().to_bytes();

    let digest = Ripemd160::digest(Sha256::digest(public_key_bytes));
    digest[..4].try_into().expect("digest truncated")
}

#[derive(Clone)]
pub struct HDWalletInner {
    /// Derived public key
    public_key: secp256k1::PublicKey,

    /// Extended key attributes.
    attrs: ExtendedKeyAttrs,

    #[allow(dead_code)]
    fingerprint: KeyFingerprint,

    hmac: HmacSha512,

    index: Arc<Mutex<u32>>
}

impl HDWalletInner {

    pub fn new(
        public_key: secp256k1::PublicKey,
        attrs: ExtendedKeyAttrs,
        fingerprint: KeyFingerprint,
        hmac: HmacSha512,
        index: u32
    )->Result<Self>{
        let wallet = Self{
            public_key,
            attrs,
            fingerprint,
            hmac,
            index: Arc::new(Mutex::new(index))
        };

        Ok(wallet)
    }

    pub async fn new_address(&self) -> Result<Address> {
        self.set_index(self.index()? + 1)?;
        self.current_address().await
    }

    pub fn index(&self)->Result<u32>{
        Ok(*self.index.lock()?)
    }

    pub fn set_index(&self, index:u32)->Result<()>{
        *self.index.lock()? = index;
        Ok(())
    }

    pub async fn current_address(&self) -> Result<Address> {
        let index = self.index()?;
        let address = self.derive_address(index).await?;

        Ok(address)
    }
    pub async fn derive_address(&self, index: u32) -> Result<Address> {
        let (key, _chain_code) =
            HDWalletGen1::derive_public_key_child(&self.public_key, index, self.hmac.clone())?;

        let pubkey = &key.to_bytes()[1..];
        let address = Address {
            prefix: AddressPrefix::Mainnet,
            version: 0,
            payload: pubkey.to_vec(),
        };

        Ok(address)
    }

    pub fn public_key(&self) -> ExtendedPublicKey<secp256k1::PublicKey> {
        self.into()
    }

    pub fn attrs(&self) -> &ExtendedKeyAttrs {
        &self.attrs
    }

    /// Serialize the raw public key as a byte array.
    pub fn to_bytes(&self) -> PublicKeyBytes {
        self.public_key().to_bytes()
    }

    /// Serialize this key as an [`ExtendedKey`].
    pub fn to_extended_key(&self, prefix: Prefix) -> ExtendedKey {
        let mut key_bytes = [0u8; KEY_SIZE + 1];
        key_bytes[..].copy_from_slice(&self.to_bytes());
        ExtendedKey {
            prefix,
            attrs: self.attrs.clone(),
            key_bytes,
        }
    }

    pub fn to_string(&self) -> Zeroizing<String> {
        Zeroizing::new(self.to_extended_key(Prefix::KPUB).to_string())
    }
}

impl From<&HDWalletInner> for ExtendedPublicKey<secp256k1::PublicKey> {
    fn from(inner: &HDWalletInner) -> ExtendedPublicKey<secp256k1::PublicKey> {
        ExtendedPublicKey {
            public_key: inner.public_key,
            attrs: inner.attrs().clone(),
        }
    }
}

#[derive(Clone)]
pub struct HDWalletGen1 {
    /// extended public key derived upto `m/<Purpose>'/111111'/<Account Index>'`
    extended_public_key: ExtendedPublicKey<secp256k1::PublicKey>,

    /// receive address wallet
    receive_wallet: HDWalletInner,

    /// change address wallet
    change_wallet: HDWalletInner,
}

impl HDWalletGen1 {
    /// build wallet from root/master private key
    pub async fn from_master_xprv(
        xprv: &str,
        is_multisig: bool,
        account_index: u64,
    ) -> Result<Self> {
        let xprv_key = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?;
        let attrs = xprv_key.attrs();

        let (extended_private_key, attrs) = Self::create_extended_key(
            *xprv_key.private_key(),
            attrs.clone(),
            is_multisig,
            account_index,
        )
        .await?;

        let extended_public_key = ExtendedPublicKey {
            public_key: extended_private_key.get_public_key(),
            attrs,
        };

        let wallet = Self::from_extended_public_key(extended_public_key).await?;

        Ok(wallet)
    }

    pub async fn from_extended_public_key_str(xpub: &str) -> Result<Self> {
        let extended_public_key = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(xpub)?;
        let wallet = Self::from_extended_public_key(extended_public_key).await?;
        Ok(wallet)
    }

    pub async fn from_extended_public_key(
        extended_public_key: ExtendedPublicKey<secp256k1::PublicKey>,
    ) -> Result<Self> {
        let receive_wallet =
            Self::derive_wallet(extended_public_key.clone(), AddressType::Receive).await?;

        let change_wallet =
            Self::derive_wallet(extended_public_key.clone(), AddressType::Change).await?;

        let wallet = Self {
            extended_public_key,
            receive_wallet,
            change_wallet,
        };

        Ok(wallet)
    }

    async fn create_extended_key(
        mut private_key: SecretKey,
        mut attrs: ExtendedKeyAttrs,
        is_multisig: bool,
        account_index: u64,
    ) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        let purpose = if is_multisig { 45 } else { 44 };
        let address_path = format!("{}'/111111'/{}'", purpose, account_index);
        let children = address_path.split('/');
        for child in children {
            (private_key, attrs) =
                Self::derive_private_key(&private_key, &attrs, child.parse::<ChildNumber>()?)
                    .await?;
        }

        Ok((private_key, attrs))
    }

    pub fn receive_wallet(&self) -> &HDWalletInner {
        &self.receive_wallet
    }
    pub fn change_wallet(&self) -> &HDWalletInner {
        &self.change_wallet
    }

    #[allow(dead_code)]
    pub async fn derive_address(&self, address_type: AddressType, index: u32) -> Result<Address> {
        let address = match address_type {
            AddressType::Receive => self.receive_wallet.derive_address(index),
            AddressType::Change => self.change_wallet.derive_address(index),
        }
        .await?;

        Ok(address)
    }

    #[inline(always)]
    pub async fn derive_receive_address(&self, index: u32) -> Result<Address> {
        let address = self.receive_wallet.derive_address(index).await?;
        Ok(address)
    }

    #[inline(always)]
    pub async fn derive_change_address(&self, index: u32) -> Result<Address> {
        let address = self.change_wallet.derive_address(index).await?;
        Ok(address)
    }

    pub async fn derive_wallet(
        mut public_key: ExtendedPublicKey<secp256k1::PublicKey>,
        address_type: AddressType,
    ) -> Result<HDWalletInner> {
        public_key = public_key.derive_child(ChildNumber::new(address_type.index(), false)?)?;

        let mut hmac = HmacSha512::new_from_slice(&public_key.attrs().chain_code)
            .map_err(Error::Hmac)?;
        hmac.update(&public_key.to_bytes());

        HDWalletInner::new(
            *public_key.public_key(),
            public_key.attrs().clone(),
            public_key.fingerprint(),
            hmac,
            0
        )
    }

    pub async fn derive_public_key(
        public_key: &secp256k1::PublicKey,
        attrs: &ExtendedKeyAttrs,
        index: u32,
    ) -> Result<(secp256k1::PublicKey, ExtendedKeyAttrs)> {
        let fingerprint = public_key.fingerprint();

        let mut hmac = HmacSha512::new_from_slice(&attrs.chain_code).map_err(Error::Hmac)?;
        hmac.update(&public_key.to_bytes());

        let (private_key, chain_code) = Self::derive_public_key_child(public_key, index, hmac)?;

        let depth = attrs.depth.checked_add(1).ok_or(Error::Depth)?;

        let attrs = ExtendedKeyAttrs {
            parent_fingerprint: fingerprint,
            child_number: ChildNumber::new(index, false)?,
            chain_code,
            depth,
        };

        Ok((private_key, attrs))
    }

    fn derive_public_key_child(
        key: &secp256k1::PublicKey,
        index: u32,
        mut hmac: HmacSha512,
    ) -> Result<(secp256k1::PublicKey, ChainCode)> {
        let child_number = ChildNumber::new(index, false)?;
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
        let key = key.derive_child(child_key.try_into()?)?;

        Ok((key, chain_code.try_into()?))
    }

    pub async fn derive_private_key(
        private_key: &SecretKey,
        attrs: &ExtendedKeyAttrs,
        child_number: ChildNumber,
    ) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        let fingerprint = get_fingerprint(private_key);

        let hmac = Self::create_hmac(private_key, attrs, child_number.is_hardened())?;

        let (private_key, chain_code) = Self::derive_key(private_key, child_number, hmac)?;

        let depth = attrs.depth.checked_add(1).ok_or(Error::Depth)?;

        let attrs = ExtendedKeyAttrs {
            parent_fingerprint: fingerprint,
            child_number,
            chain_code,
            depth,
        };

        Ok((private_key, attrs))
    }

    fn derive_key(
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

    pub fn create_hmac<K>(
        private_key: &K,
        attrs: &ExtendedKeyAttrs,
        hardened: bool,
    ) -> Result<HmacSha512>
    where
        K: PrivateKey<PublicKey = secp256k1::PublicKey>,
    {
        let mut hmac = HmacSha512::new_from_slice(&attrs.chain_code).map_err(Error::Hmac)?;
        if hardened {
            hmac.update(&[0]);
            hmac.update(&private_key.to_bytes());
        } else {
            hmac.update(&private_key.public_key().to_bytes());
        }

        Ok(hmac)
    }

    /// Serialize the raw public key as a byte array.
    pub fn to_bytes(&self) -> PublicKeyBytes {
        self.extended_public_key.to_bytes()
    }

    pub fn attrs(&self) -> &ExtendedKeyAttrs {
        self.extended_public_key.attrs()
    }

    /// Serialize this key as a self-[`Zeroizing`] `String`.
    pub fn to_string(&self) -> Zeroizing<String> {
        let key = self.extended_public_key.to_string(Some(Prefix::KPUB));
        Zeroizing::new(key)
    }
}

impl Debug for HDWalletGen1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HDWallet")
            .field("depth", &self.attrs().depth)
            .field("child_number", &self.attrs().child_number)
            .field("chain_code", &faster_hex::hex_string(&self.attrs().chain_code))
            .field("public_key", &faster_hex::hex_string(&self.to_bytes()))
            .field("parent_fingerprint", &self.attrs().parent_fingerprint)
            .finish()
    }
}
