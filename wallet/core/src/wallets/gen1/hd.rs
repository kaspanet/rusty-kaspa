use addresses::{Address, Prefix as AddressPrefix, Version};
use hmac::Mac;
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use std::{
    fmt::Debug,
    str::FromStr,
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

use kaspa_bip32::{
    types::*, AddressType, ChildNumber, ExtendedKey, ExtendedKeyAttrs, ExtendedPrivateKey, ExtendedPublicKey, Prefix, PrivateKey,
    PublicKey, SecretKey, SecretKeyExt,
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

    index: Arc<Mutex<u32>>,
}

impl HDWalletInner {
    pub fn new(
        public_key: secp256k1::PublicKey,
        attrs: ExtendedKeyAttrs,
        fingerprint: KeyFingerprint,
        hmac: HmacSha512,
        index: u32,
    ) -> Result<Self> {
        let wallet = Self { public_key, attrs, fingerprint, hmac, index: Arc::new(Mutex::new(index)) };

        Ok(wallet)
    }

    pub async fn new_address(&self) -> Result<Address> {
        self.set_index(self.index()? + 1)?;
        self.current_address().await
    }

    pub fn index(&self) -> Result<u32> {
        Ok(*self.index.lock()?)
    }

    pub fn set_index(&self, index: u32) -> Result<()> {
        *self.index.lock()? = index;
        Ok(())
    }

    pub async fn current_address(&self) -> Result<Address> {
        let index = self.index()?;
        let address = self.derive_address(index).await?;

        Ok(address)
    }
    pub async fn derive_address(&self, index: u32) -> Result<Address> {
        let (key, _chain_code) = HDWalletGen1::derive_public_key_child(&self.public_key, index, self.hmac.clone())?;

        let pubkey = &key.to_bytes()[1..];
        let address = Address::new(AddressPrefix::Mainnet, Version::PubKey, pubkey);

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
        ExtendedKey { prefix, attrs: self.attrs.clone(), key_bytes }
    }

    pub fn to_string(&self) -> Zeroizing<String> {
        Zeroizing::new(self.to_extended_key(Prefix::KPUB).to_string())
    }
}

impl From<&HDWalletInner> for ExtendedPublicKey<secp256k1::PublicKey> {
    fn from(inner: &HDWalletInner) -> ExtendedPublicKey<secp256k1::PublicKey> {
        ExtendedPublicKey { public_key: inner.public_key, attrs: inner.attrs().clone() }
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
    pub async fn from_master_xprv(xprv: &str, is_multisig: bool, account_index: u64) -> Result<Self> {
        let xprv_key = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?;
        let attrs = xprv_key.attrs();

        let (extended_private_key, attrs) =
            Self::create_extended_key(*xprv_key.private_key(), attrs.clone(), is_multisig, account_index).await?;

        let extended_public_key = ExtendedPublicKey { public_key: extended_private_key.get_public_key(), attrs };

        let wallet = Self::from_extended_public_key(extended_public_key).await?;

        Ok(wallet)
    }

    pub async fn from_extended_public_key_str(xpub: &str) -> Result<Self> {
        let extended_public_key = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(xpub)?;
        let wallet = Self::from_extended_public_key(extended_public_key).await?;
        Ok(wallet)
    }

    pub async fn from_extended_public_key(extended_public_key: ExtendedPublicKey<secp256k1::PublicKey>) -> Result<Self> {
        let receive_wallet = Self::derive_wallet(extended_public_key.clone(), AddressType::Receive).await?;

        let change_wallet = Self::derive_wallet(extended_public_key.clone(), AddressType::Change).await?;

        let wallet = Self { extended_public_key, receive_wallet, change_wallet };

        Ok(wallet)
    }

    async fn create_extended_key(
        mut private_key: SecretKey,
        mut attrs: ExtendedKeyAttrs,
        is_multisig: bool,
        account_index: u64,
    ) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        let purpose = if is_multisig { 45 } else { 44 };
        let address_path = format!("{purpose}'/111111'/{account_index}'");
        let children = address_path.split('/');
        for child in children {
            (private_key, attrs) = Self::derive_private_key(&private_key, &attrs, child.parse::<ChildNumber>()?).await?;
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

        let mut hmac = HmacSha512::new_from_slice(&public_key.attrs().chain_code).map_err(Error::Hmac)?;
        hmac.update(&public_key.to_bytes());

        HDWalletInner::new(*public_key.public_key(), public_key.attrs().clone(), public_key.fingerprint(), hmac, 0)
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

        let attrs =
            ExtendedKeyAttrs { parent_fingerprint: fingerprint, child_number: ChildNumber::new(index, false)?, chain_code, depth };

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

        let attrs = ExtendedKeyAttrs { parent_fingerprint: fingerprint, child_number, chain_code, depth };

        Ok((private_key, attrs))
    }

    fn derive_key(private_key: &SecretKey, child_number: ChildNumber, mut hmac: HmacSha512) -> Result<(SecretKey, ChainCode)> {
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

    pub fn create_hmac<K>(private_key: &K, attrs: &ExtendedKeyAttrs, hardened: bool) -> Result<HmacSha512>
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

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::HDWalletGen1;

    fn gen1_receive_addresses() -> Vec<String> {
        vec![
            "kaspa:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjellj43pf".to_string(),
            "kaspa:qzn3qjzf2nzyd3zj303nk4sgv0aae42v3ufutk5xsxckfels57dxjjed4qvlx".to_string(),
            "kaspa:qpakxqlesqywgkq7rg4wyhjd93kmw7trkl3gpa3vd5flyt59a43yyjp28qsku".to_string(),
            "kaspa:qz0skffpert8cav6h2c9nfndmhzzfhvkrjexclmwgjjwt0sutysnw6lp55ak0".to_string(),
            "kaspa:qrmzemw6sm67svltul0qsk3974ema4auhrja3k68f4sfhxe4mxjwx0cj353df".to_string(),
            "kaspa:qpe4apax5dquy600py9rprmukhq8fqyqv9qu072twkvgse0glhqa74ynxmvfr".to_string(),
            "kaspa:qrptdge6ykdq672xqjd4rv2cedwdcz030jngsr2xhaxrn5l8pfhc294x9c7x6".to_string(),
            "kaspa:qqnys5nyennjkvyl77vwneq5j2vmjss57zerd88ptzaeqhm998smxw28uth8l".to_string(),
            "kaspa:qztckuvk02885rdazvj9w079qujg5qpxcdnmsvxqx0q8z7l483prkszjqwwff".to_string(),
            "kaspa:qrp53krck4m0x6n0dxs7vzf5mg0x6we8e06xjpmu8xr8p4du6f89khqdzw6uw".to_string(),
            "kaspa:qr4l3mahqe0jeeu6c474q5tywz08mudhddgtdneeq46unv0qx0j77kdtr52uu".to_string(),
            "kaspa:qzatdsueklx7pkfzanh9u0pwr47sd3a25gfm8wypsevdejhhpj8ck3v74v54j".to_string(),
            "kaspa:qqk3g5l6ymdkjfmzezx4zrv9fhr5rh0d8tm07udkqxq79n6t60tzu3fa7lnqg".to_string(),
            "kaspa:qqasa6d590u6875hsese68fa9f8mnedzesn2udehp0s73ggt5cklw2ge393eq".to_string(),
            "kaspa:qpuzq5jc757uxue9fradme33jd6egxr9fdznd8qysqcc5xy8k7alqpjgpdgrn".to_string(),
            "kaspa:qqygznwmkl56vprrnvyvnta9qql43yv52m3qz2462vxskn32axl0xccnpsqx9".to_string(),
            "kaspa:qqk974yml6uuustenwu57hn8n7d202luvn4dum0txvzjgg60g2jzsknngheak".to_string(),
            "kaspa:qpxqat995cxnjla8nm0dwnneesqnk5enc6hqrua7jztels0eqjg8vsm032lww".to_string(),
            "kaspa:qpyzkjs2a6k8ljx2qt4pwscj6jccr6k7pmru9k7r2t25teajjuzaz7zkesu0e".to_string(),
            "kaspa:qzf5mxtvk8wgp8gr3dcj3dkzdu6w4dgpvp2f0gm9pepv9vazxrhy577fy87rt".to_string(),
            "kaspa:qz44rhjkrddak9vf5z4swlmenxtfhmqc47d0lyf0j7ednyjln0u824ue33gvr".to_string(),
        ]
    }

    fn gen1_change_addresses() -> Vec<String> {
        vec![
            "kaspa:qrqrnyzdwh9ec2q05guzy3vv33f86nvdyw52qwlmk0mewzx3dgdss3pmcd692".to_string(),
            "kaspa:qqx8jlz0hh0wun5ru4glt9za3v8wj3jn7v3w55a0lyud74ppetqfqny4yhw87".to_string(),
            "kaspa:qzpa69mrh2nj6xk6gq38vcnzu64necp0jwaxxyusr9xcy5udhu2m7uvql8rnd".to_string(),
            "kaspa:qqxddf76hr39dc7k7lpdzg065ajtvrhlm5p3edm4gyen0waneryss2c0la85t".to_string(),
            "kaspa:qps4qh9dtskwvf923yl9utl74r8sdm9h2wv3mftuxcfc2cshwswc6txj0k2kl".to_string(),
            "kaspa:qrds58d6nw9uz7z93ds4l6x9cgw3rquqzr69dtch6n4d8fxum8c65f7nqmhzx".to_string(),
            "kaspa:qrajjrpj0krqkww7rymwuwzcd36grjr6688ynvna649q26zukhcq6eqf4jmnx".to_string(),
            "kaspa:qrumkgz7hlsa748tnzvpztmf6wu9zsgqh6rppw4gzw2mvyq4ccj0y3ms9ju5l".to_string(),
            "kaspa:qz2g3cj3jcklk4w95djwnm9dffcwg75aqct2pefsxujrldgs08wac99rz70rc".to_string(),
            "kaspa:qznmzsvk0srfkur8l9pf55st0hnh3x8tmdyskjl9570w99lxsgs7cwrhxap2r".to_string(),
            "kaspa:qptamza95k7tchmukulldps4kl6wk853dnwa52t4azzm76h588qjufmnu3rn7".to_string(),
            "kaspa:qqt9h5cjqu9an68cn9k9jc2ywqmqu6kswjzeu09tqulswxkuccaxg6wz45f5r".to_string(),
            "kaspa:qphr6uy46ad3ca7rerzkx7kkzfzsvfe0xanh4u5mrh538cexs4yjkww0pa4dh".to_string(),
            "kaspa:qzv3qlh5q4fpy6eu5s4wj080l64del4lvg986z5uh0c3g7wf6n8pvsgm3c9e0".to_string(),
            "kaspa:qp2dd6y4szgyhcendh7ncxws0qvx8k3s92tg7lvy8eel5npg4pd2ks0ctx4hl".to_string(),
            "kaspa:qpkqvnkler4rwlpt720unepf3q8cayv0shx0vzydrae7a6u7ryy8zdvnmncyc".to_string(),
            "kaspa:qr4v33jupxv9h6juqads0znrnw6g7an2ajuzusthnjqujquz66rewtjekhz4l".to_string(),
            "kaspa:qz5pq2yzpz8ce5avrsa4uzzwrlr5a86rvs74afd6qdm3h649v08nk0qxhrl9n".to_string(),
            "kaspa:qrajmn035raezl6rcvd0wvnfmdnc0qzwr686ccsrn3z5x8aqnpt8qa0e954jk".to_string(),
            "kaspa:qrqg7r05nk7syxjh8rdz8wanzmyh8sdts9uexxnnwkq8fplrjammvcnrdggw0".to_string(),
        ]
    }

    #[tokio::test]
    async fn hd_wallet_gen1() {
        let master_xprv =
            "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";

        let hd_wallet = HDWalletGen1::from_master_xprv(master_xprv, false, 0).await;
        assert!(hd_wallet.is_ok(), "Could not parse key");
        let hd_wallet = hd_wallet.unwrap();

        let receive_addresses = gen1_receive_addresses();
        let change_addresses = gen1_change_addresses();

        for index in 0..20 {
            let address: String = hd_wallet.derive_receive_address(index).await.unwrap().into();
            assert_eq!(receive_addresses[index as usize], address, "receive address at {index} failed");
            let address: String = hd_wallet.derive_change_address(index).await.unwrap().into();
            assert_eq!(change_addresses[index as usize], address, "change address at {index} failed");
        }
    }
}
