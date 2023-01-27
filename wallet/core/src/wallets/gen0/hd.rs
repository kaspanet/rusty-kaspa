use addresses::{Address, Prefix as AddressPrefix};
use hmac::Mac;
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use std::fmt::Debug;
use std::str::FromStr;
use zeroize::Zeroizing;
//use secp256k1_ffi::SECP256K1;

//use workflow_core::task::*;
//use std::time::Duration;
use kaspa_bip32::{
    types::*, AddressType, ChildNumber, ExtendedKey, ExtendedKeyAttrs, ExtendedPrivateKey,
    ExtendedPublicKey, Prefix, PrivateKey, PublicKey, SecretKey, SecretKeyExt,
};

#[derive(Clone)]
pub struct HDWalletInner {
    /// Derived private key
    private_key: SecretKey,

    /// Extended key attributes.
    attrs: ExtendedKeyAttrs,

    #[allow(dead_code)]
    fingerprint: [u8; 4],

    hmac: HmacSha512,
}

impl HDWalletInner {
    pub async fn derive_address(&self, index: u32) -> Result<Address> {
        //let child_number = ChildNumber::new(index, true)?;
        //key = key.derive_child(c.clone())?;
        //println!("\nc:    {c:?}");
        //println!("key: {:#?}", key2);
        /*
        println!("key: {:?}",
            //key.to_string(Prefix::XPRV).as_str(),
            key2.to_string(Prefix::XPRV).as_str()
        );
        */
        //key = key.derive_child(child_number).await?;
        /*
        let (private_key, _attrs) = HDWalletGen0::derive_child_with_fingerprint(
            &self.private_key,
            &self.attrs,
            child_number,
            self.fingerprint,
            self.hmac.clone()
        ).await?;
        */

        let (private_key, _) = HDWalletGen0::derive_private_key(
            &self.private_key,
            ChildNumber::new(index, true)?,
            self.hmac.clone(),
        )?;
        //sleep(Duration::from_secs(0)).await;
        //yield_now().await;

        /*
        let key = ExtendedPublicKey {
            public_key: private_key.public_key(),
            attrs
        };
        */

        let pubkey = &private_key.get_public_key().to_bytes()[1..];
        //let pubkey = &private_key.public_key().to_bytes()[1..];
        let address = Address {
            prefix: AddressPrefix::Mainnet,
            version: 0,
            payload: pubkey.to_vec(),
        };

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

impl From<&HDWalletInner> for ExtendedPublicKey<<SecretKey as PrivateKey>::PublicKey> {
    fn from(inner: &HDWalletInner) -> ExtendedPublicKey<<SecretKey as PrivateKey>::PublicKey> {
        ExtendedPublicKey {
            public_key: inner.private_key().get_public_key(),
            //public_key: inner.private_key().public_key(),
            attrs: inner.attrs().clone(),
        }
    }
}

#[derive(Clone)]
pub struct HDWalletGen0 {
    /// Derived private key
    private_key: SecretKey,

    /// Extended key attributes.
    attrs: ExtendedKeyAttrs,

    receive_wallet: HDWalletInner,
    change_wallet: HDWalletInner,
}

impl HDWalletGen0 {
    pub async fn from_str(xpriv: &str) -> Result<Self> {
        let xpriv_key = ExtendedPrivateKey::<SecretKey>::from_str(xpriv)?;
        let attrs = xpriv_key.attrs();

        let receive_wallet = Self::derive_wallet(
            *xpriv_key.private_key(),
            attrs.clone(),
            AddressType::Receive,
        )
        .await?;

        let change_wallet =
            Self::derive_wallet(*xpriv_key.private_key(), attrs.clone(), AddressType::Change)
                .await?;

        let wallet = Self {
            private_key: *xpriv_key.private_key(),
            attrs: attrs.clone(),
            receive_wallet,
            change_wallet,
        };

        Ok(wallet)
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
        mut private_key: SecretKey,
        mut attrs: ExtendedKeyAttrs,
        address_type: AddressType,
    ) -> Result<HDWalletInner> {
        let address_path = format!("44'/972/0'/{}'", address_type.index());
        let children = address_path.split('/');
        for child in children {
            let c = child.parse::<ChildNumber>()?;
            //key = key.derive_child(c.clone())?;
            //println!("\nc:    {c:?}");
            //println!("key: {:#?}", key2);
            /*
            println!("key: {:?}",
                //key.to_string(Prefix::XPRV).as_str(),
                key2.to_string(Prefix::XPRV).as_str()
            );
            */
            (private_key, attrs) = Self::derive_child(&private_key, &attrs, c).await?;
            //sleep(Duration::from_secs(0)).await;
            //yield_now().await
        }

        let public_key_bytes = &private_key.get_public_key().to_bytes()[1..];

        let digest = Ripemd160::digest(Sha256::digest(public_key_bytes));
        let fingerprint = digest[..4].try_into().expect("digest truncated");

        let hmac = Self::create_hmac(&private_key, &attrs, true)?;

        Ok(HDWalletInner {
            private_key,
            attrs,
            fingerprint,
            hmac,
        })
    }

    pub async fn derive_child(
        private_key: &SecretKey,
        attrs: &ExtendedKeyAttrs,
        child_number: ChildNumber,
    ) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        let public_key_bytes = &private_key.get_public_key().to_bytes()[1..];

        let digest = Ripemd160::digest(Sha256::digest(public_key_bytes));
        //let digest = Ripemd160::digest(&Sha256::digest(&private_key.public_key().to_bytes()[1..]));
        let fingerprint = digest[..4].try_into().expect("digest truncated");

        let hmac = Self::create_hmac(private_key, attrs, child_number.is_hardened())?;

        let res = Self::derive_child_with_fingerprint(
            private_key,
            attrs,
            child_number,
            fingerprint,
            hmac,
        )
        .await?;

        Ok(res)
    }

    pub fn create_hmac(
        private_key: &SecretKey,
        attrs: &ExtendedKeyAttrs,
        hardened: bool,
    ) -> Result<HmacSha512> {
        let mut hmac = HmacSha512::new_from_slice(&attrs.chain_code).map_err(Error::Hmac)?;
        if hardened {
            hmac.update(&[0]);
            hmac.update(&private_key.to_bytes());
            //println!("data: {}{}", hex::encode(self.private_key.to_bytes()), hex::encode(child_number.to_bytes()));
        } else {
            let public_key_bytes = &private_key.get_public_key().to_bytes()[1..];
            hmac.update(public_key_bytes);
            //hmac.update(&private_key.public_key().to_bytes()[1..]);
            //println!("data: {}{}", hex::encode(&self.private_key.public_key().to_bytes()[1..]), hex::encode(child_number.to_bytes()));
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

        let attrs = ExtendedKeyAttrs {
            parent_fingerprint: fingerprint,
            child_number,
            chain_code,
            depth,
        };

        let derived = (private_key, attrs);

        /*
        derived.log(Prefix::XPRV);

        println!("index:{}\nderived.childIndex:{}\nderived.xprivkey:{}",
            child_number,
            derived.attrs.child_number,
            derived.to_string(Prefix::XPRV).as_str()
        );

        println!("==================\n");
        */

        Ok(derived)
    }

    pub fn derive_private_key(
        private_key: &SecretKey,
        child_number: ChildNumber,
        mut hmac: HmacSha512,
    ) -> Result<(SecretKey, ChainCode)> {
        /*
        println!("\n private_key: {}", hex::encode(private_key.to_bytes()));

        println!("\n_deriveWithNumber {}, {}, {}, fingerprint:{}",
            child_number,
            child_number.is_hardened(),
            hex::encode(private_key.get_public_key().to_bytes()),
            hex::encode(&fingerprint)
        );
        */

        hmac.update(&child_number.to_bytes());

        let result = hmac.finalize().into_bytes();
        //println!("chainCode: {}", hex::encode(self.attrs.chain_code));
        //println!("hash: {}", hex::encode(&result));

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

        ExtendedKey {
            prefix,
            attrs: self.attrs.clone(),
            key_bytes,
        }
    }

    /// Serialize this key as a self-[`Zeroizing`] `String`.
    pub fn to_string(&self) -> Zeroizing<String> {
        let key = self.to_extended_key(Prefix::XPRV);

        //println!("str: {str}");
        Zeroizing::new(key.to_string())
    }

    /*
    pub fn log(&self, prefix: Prefix){
        let key = self.to_extended_key(prefix);
        let mut buf = [0u8; ExtendedKey::MAX_BASE58_SIZE];
        let _str = write_base58(&key, &mut buf)
            .map_err(|_| std::fmt::Error).unwrap();
    }
    */

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

impl From<&HDWalletGen0> for ExtendedPublicKey<<SecretKey as PrivateKey>::PublicKey> {
    fn from(hd_wallet: &HDWalletGen0) -> ExtendedPublicKey<<SecretKey as PrivateKey>::PublicKey> {
        ExtendedPublicKey {
            public_key: hd_wallet.private_key().get_public_key(),
            //public_key: hd_wallet.private_key().public_key(),
            attrs: hd_wallet.attrs().clone(),
        }
    }
}

impl Debug for HDWalletGen0 {
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
