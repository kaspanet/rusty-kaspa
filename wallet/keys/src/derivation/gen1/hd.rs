use crate::derivation::traits::*;
use crate::imports::*;
use hmac::Mac;
use kaspa_addresses::{Address, Prefix as AddressPrefix, Version as AddressVersion};
use kaspa_bip32::types::{ChainCode, HmacSha512, KeyFingerprint, PublicKeyBytes, KEY_SIZE};
use kaspa_bip32::{
    AddressType, ChildNumber, DerivationPath, ExtendedKey, ExtendedKeyAttrs, ExtendedPrivateKey, ExtendedPublicKey, Prefix,
    PrivateKey, PublicKey, SecretKey, SecretKeyExt,
};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use std::fmt::Debug;
// use wasm_bindgen::prelude::*;

fn get_fingerprint<K>(private_key: &K) -> KeyFingerprint
where
    K: PrivateKey,
{
    let public_key_bytes = private_key.public_key().to_bytes();

    let digest = Ripemd160::digest(Sha256::digest(public_key_bytes));
    digest[..4].try_into().expect("digest truncated")
}

#[derive(Clone)]
// #[wasm_bindgen(inspectable)]
pub struct PubkeyDerivationManager {
    /// Derived public key
    public_key: secp256k1::PublicKey,
    /// Extended key attributes.
    attrs: ExtendedKeyAttrs,
    #[allow(dead_code)]
    fingerprint: KeyFingerprint,
    hmac: HmacSha512,
    index: Arc<Mutex<u32>>,
}

impl PubkeyDerivationManager {
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

    pub fn derive_pubkey_range(&self, indexes: std::ops::Range<u32>) -> Result<Vec<secp256k1::PublicKey>> {
        let list = indexes.map(|index| self.derive_pubkey(index)).collect::<Vec<_>>();
        let keys = list.into_iter().collect::<Result<Vec<_>>>()?;
        Ok(keys)
    }

    pub fn derive_pubkey(&self, index: u32) -> Result<secp256k1::PublicKey> {
        let (key, _chain_code) = WalletDerivationManager::derive_public_key_child(&self.public_key, index, self.hmac.clone())?;
        Ok(key)
    }

    pub fn create_address(key: &secp256k1::PublicKey, prefix: AddressPrefix, ecdsa: bool) -> Result<Address> {
        let address = if ecdsa {
            let payload = &key.serialize();
            Address::new(prefix, AddressVersion::PubKeyECDSA, payload)
        } else {
            let payload = &key.x_only_public_key().0.serialize();
            Address::new(prefix, AddressVersion::PubKey, payload)
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
        ExtendedKey { prefix, attrs: self.attrs.clone(), key_bytes }
    }

    pub fn to_string(&self) -> Zeroizing<String> {
        Zeroizing::new(self.to_extended_key(Prefix::KPUB).to_string())
    }
}

// #[wasm_bindgen]
impl PubkeyDerivationManager {
    // #[wasm_bindgen(getter, js_name = publicKey)]
    pub fn get_public_key(&self) -> String {
        self.public_key().to_string(None)
    }
}

impl From<&PubkeyDerivationManager> for ExtendedPublicKey<secp256k1::PublicKey> {
    fn from(inner: &PubkeyDerivationManager) -> ExtendedPublicKey<secp256k1::PublicKey> {
        ExtendedPublicKey { public_key: inner.public_key, attrs: inner.attrs().clone() }
    }
}

#[async_trait]
impl PubkeyDerivationManagerTrait for PubkeyDerivationManager {
    fn new_pubkey(&self) -> Result<secp256k1::PublicKey> {
        self.set_index(self.index()? + 1)?;
        self.current_pubkey()
    }

    fn index(&self) -> Result<u32> {
        Ok(*self.index.lock()?)
    }

    fn set_index(&self, index: u32) -> Result<()> {
        *self.index.lock()? = index;
        Ok(())
    }

    fn current_pubkey(&self) -> Result<secp256k1::PublicKey> {
        let index = self.index()?;
        let key = self.derive_pubkey(index)?;

        Ok(key)
    }

    fn get_range(&self, range: std::ops::Range<u32>) -> Result<Vec<secp256k1::PublicKey>> {
        self.derive_pubkey_range(range)
    }
}

#[derive(Clone)]
pub struct WalletDerivationManager {
    /// extended public key derived upto `m/<Purpose>'/111111'/<Account Index>'`
    extended_public_key: ExtendedPublicKey<secp256k1::PublicKey>,

    /// receive address wallet
    receive_pubkey_manager: Arc<PubkeyDerivationManager>,

    /// change address wallet
    change_pubkey_manager: Arc<PubkeyDerivationManager>,
}

impl WalletDerivationManager {
    pub fn create_extended_key_from_xprv(xprv: &str, is_multisig: bool, account_index: u64) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        let xprv_key = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?;
        Self::derive_extended_key_from_master_key(xprv_key, is_multisig, account_index)
    }

    pub fn derive_extended_key_from_master_key(
        xprv_key: ExtendedPrivateKey<SecretKey>,
        is_multisig: bool,
        account_index: u64,
    ) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        let attrs = xprv_key.attrs();

        let (extended_private_key, attrs) =
            Self::create_extended_key(*xprv_key.private_key(), attrs.clone(), is_multisig, account_index)?;

        Ok((extended_private_key, attrs))
    }

    fn create_extended_key(
        mut private_key: SecretKey,
        mut attrs: ExtendedKeyAttrs,
        is_multisig: bool,
        account_index: u64,
    ) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        let purpose = if is_multisig { 45 } else { 44 };
        let address_path = format!("{purpose}'/111111'/{account_index}'");
        let children = address_path.split('/');
        for child in children {
            (private_key, attrs) = Self::derive_private_key(&private_key, &attrs, child.parse::<ChildNumber>()?)?;
        }

        Ok((private_key, attrs))
    }

    pub fn build_derivate_path(
        is_multisig: bool,
        account_index: u64,
        cosigner_index: Option<u32>,
        address_type: Option<AddressType>,
    ) -> Result<DerivationPath> {
        if is_multisig && cosigner_index.is_none() {
            return Err("cosigner_index is required for multisig path derivation".to_string().into());
        }
        let purpose = if is_multisig { 45 } else { 44 };
        let mut path = format!("m/{purpose}'/111111'/{account_index}'");
        if let Some(cosigner_index) = cosigner_index {
            path = format!("{path}/{}", cosigner_index)
        }
        if let Some(address_type) = address_type {
            path = format!("{path}/{}", address_type.index());
        }
        let path = path.parse::<DerivationPath>()?;
        Ok(path)
    }

    pub fn receive_pubkey_manager(&self) -> &PubkeyDerivationManager {
        &self.receive_pubkey_manager
    }
    pub fn change_pubkey_manager(&self) -> &PubkeyDerivationManager {
        &self.change_pubkey_manager
    }

    pub fn derive_child_pubkey_manager(
        mut public_key: ExtendedPublicKey<secp256k1::PublicKey>,
        address_type: AddressType,
        cosigner_index: Option<u32>,
    ) -> Result<PubkeyDerivationManager> {
        if let Some(cosigner_index) = cosigner_index {
            public_key = public_key.derive_child(ChildNumber::new(cosigner_index, false)?)?;
        }

        public_key = public_key.derive_child(ChildNumber::new(address_type.index(), false)?)?;

        let mut hmac = HmacSha512::new_from_slice(&public_key.attrs().chain_code).map_err(kaspa_bip32::Error::Hmac)?;
        hmac.update(&public_key.to_bytes());

        PubkeyDerivationManager::new(*public_key.public_key(), public_key.attrs().clone(), public_key.fingerprint(), hmac, 0)
    }

    pub fn derive_public_key(
        public_key: &secp256k1::PublicKey,
        attrs: &ExtendedKeyAttrs,
        index: u32,
    ) -> Result<(secp256k1::PublicKey, ExtendedKeyAttrs)> {
        let fingerprint = public_key.fingerprint();

        let mut hmac = HmacSha512::new_from_slice(&attrs.chain_code).map_err(kaspa_bip32::Error::Hmac)?;
        hmac.update(&public_key.to_bytes());

        let (key, chain_code) = Self::derive_public_key_child(public_key, index, hmac)?;

        let depth = attrs.depth.checked_add(1).ok_or(kaspa_bip32::Error::Depth)?;

        let attrs =
            ExtendedKeyAttrs { parent_fingerprint: fingerprint, child_number: ChildNumber::new(index, false)?, chain_code, depth };

        Ok((key, attrs))
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

    pub fn derive_private_key(
        private_key: &SecretKey,
        attrs: &ExtendedKeyAttrs,
        child_number: ChildNumber,
    ) -> Result<(SecretKey, ExtendedKeyAttrs)> {
        let fingerprint = get_fingerprint(private_key);

        let hmac = Self::create_hmac(private_key, attrs, child_number.is_hardened())?;

        let (private_key, chain_code) = Self::derive_key(private_key, child_number, hmac)?;

        let depth = attrs.depth.checked_add(1).ok_or(kaspa_bip32::Error::Depth)?;

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
        let mut hmac = HmacSha512::new_from_slice(&attrs.chain_code).map_err(kaspa_bip32::Error::Hmac)?;
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
    pub fn to_string(&self, prefix: Option<Prefix>) -> Zeroizing<String> {
        let key = self.extended_public_key.to_string(Some(prefix.unwrap_or(Prefix::KPUB)));
        Zeroizing::new(key)
    }
}

impl Debug for WalletDerivationManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WalletAccount")
            .field("depth", &self.attrs().depth)
            .field("child_number", &self.attrs().child_number)
            .field("chain_code", &faster_hex::hex_string(&self.attrs().chain_code))
            .field("public_key", &faster_hex::hex_string(&self.to_bytes()))
            .field("parent_fingerprint", &self.attrs().parent_fingerprint)
            .finish()
    }
}

#[async_trait]
impl WalletDerivationManagerTrait for WalletDerivationManager {
    /// build wallet from root/master private key
    fn from_master_xprv(xprv: &str, is_multisig: bool, account_index: u64, cosigner_index: Option<u32>) -> Result<Self> {
        let xprv_key = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?;
        let attrs = xprv_key.attrs();

        let (extended_private_key, attrs) =
            Self::create_extended_key(*xprv_key.private_key(), attrs.clone(), is_multisig, account_index)?;

        let extended_public_key = ExtendedPublicKey { public_key: extended_private_key.get_public_key(), attrs };

        let wallet = Self::from_extended_public_key(extended_public_key, cosigner_index)?;

        Ok(wallet)
    }

    fn from_extended_public_key_str(xpub: &str, cosigner_index: Option<u32>) -> Result<Self> {
        let extended_public_key = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(xpub)?;
        let wallet = Self::from_extended_public_key(extended_public_key, cosigner_index)?;
        Ok(wallet)
    }

    fn from_extended_public_key(
        extended_public_key: ExtendedPublicKey<secp256k1::PublicKey>,
        cosigner_index: Option<u32>,
    ) -> Result<Self> {
        let receive_wallet = Self::derive_child_pubkey_manager(extended_public_key.clone(), AddressType::Receive, cosigner_index)?;

        let change_wallet = Self::derive_child_pubkey_manager(extended_public_key.clone(), AddressType::Change, cosigner_index)?;

        let wallet = Self {
            extended_public_key,
            receive_pubkey_manager: Arc::new(receive_wallet),
            change_pubkey_manager: Arc::new(change_wallet),
        };

        Ok(wallet)
    }

    fn receive_pubkey_manager(&self) -> Arc<dyn PubkeyDerivationManagerTrait> {
        self.receive_pubkey_manager.clone()
    }

    fn change_pubkey_manager(&self) -> Arc<dyn PubkeyDerivationManagerTrait> {
        self.change_pubkey_manager.clone()
    }

    #[inline(always)]
    fn new_receive_pubkey(&self) -> Result<secp256k1::PublicKey> {
        let key = self.receive_pubkey_manager.new_pubkey()?;
        Ok(key)
    }

    #[inline(always)]
    fn new_change_pubkey(&self) -> Result<secp256k1::PublicKey> {
        let key = self.change_pubkey_manager.new_pubkey()?;
        Ok(key)
    }

    #[inline(always)]
    fn receive_pubkey(&self) -> Result<secp256k1::PublicKey> {
        let key = self.receive_pubkey_manager.current_pubkey()?;
        Ok(key)
    }

    #[inline(always)]
    fn change_pubkey(&self) -> Result<secp256k1::PublicKey> {
        let key = self.change_pubkey_manager.current_pubkey()?;
        Ok(key)
    }

    #[inline(always)]
    fn derive_receive_pubkey(&self, index: u32) -> Result<secp256k1::PublicKey> {
        let key = self.receive_pubkey_manager.derive_pubkey(index)?;
        Ok(key)
    }

    #[inline(always)]
    fn derive_change_pubkey(&self, index: u32) -> Result<secp256k1::PublicKey> {
        let key = self.change_pubkey_manager.derive_pubkey(index)?;
        Ok(key)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::{PubkeyDerivationManager, WalletDerivationManager, WalletDerivationManagerTrait};
    use kaspa_addresses::Prefix;

    fn gen1_receive_addresses() -> Vec<&'static str> {
        vec![
            "kaspa:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjellj43pf",
            "kaspa:qzn3qjzf2nzyd3zj303nk4sgv0aae42v3ufutk5xsxckfels57dxjjed4qvlx",
            "kaspa:qpakxqlesqywgkq7rg4wyhjd93kmw7trkl3gpa3vd5flyt59a43yyjp28qsku",
            "kaspa:qz0skffpert8cav6h2c9nfndmhzzfhvkrjexclmwgjjwt0sutysnw6lp55ak0",
            "kaspa:qrmzemw6sm67svltul0qsk3974ema4auhrja3k68f4sfhxe4mxjwx0cj353df",
            "kaspa:qpe4apax5dquy600py9rprmukhq8fqyqv9qu072twkvgse0glhqa74ynxmvfr",
            "kaspa:qrptdge6ykdq672xqjd4rv2cedwdcz030jngsr2xhaxrn5l8pfhc294x9c7x6",
            "kaspa:qqnys5nyennjkvyl77vwneq5j2vmjss57zerd88ptzaeqhm998smxw28uth8l",
            "kaspa:qztckuvk02885rdazvj9w079qujg5qpxcdnmsvxqx0q8z7l483prkszjqwwff",
            "kaspa:qrp53krck4m0x6n0dxs7vzf5mg0x6we8e06xjpmu8xr8p4du6f89khqdzw6uw",
            "kaspa:qr4l3mahqe0jeeu6c474q5tywz08mudhddgtdneeq46unv0qx0j77kdtr52uu",
            "kaspa:qzatdsueklx7pkfzanh9u0pwr47sd3a25gfm8wypsevdejhhpj8ck3v74v54j",
            "kaspa:qqk3g5l6ymdkjfmzezx4zrv9fhr5rh0d8tm07udkqxq79n6t60tzu3fa7lnqg",
            "kaspa:qqasa6d590u6875hsese68fa9f8mnedzesn2udehp0s73ggt5cklw2ge393eq",
            "kaspa:qpuzq5jc757uxue9fradme33jd6egxr9fdznd8qysqcc5xy8k7alqpjgpdgrn",
            "kaspa:qqygznwmkl56vprrnvyvnta9qql43yv52m3qz2462vxskn32axl0xccnpsqx9",
            "kaspa:qqk974yml6uuustenwu57hn8n7d202luvn4dum0txvzjgg60g2jzsknngheak",
            "kaspa:qpxqat995cxnjla8nm0dwnneesqnk5enc6hqrua7jztels0eqjg8vsm032lww",
            "kaspa:qpyzkjs2a6k8ljx2qt4pwscj6jccr6k7pmru9k7r2t25teajjuzaz7zkesu0e",
            "kaspa:qzf5mxtvk8wgp8gr3dcj3dkzdu6w4dgpvp2f0gm9pepv9vazxrhy577fy87rt",
            "kaspa:qz44rhjkrddak9vf5z4swlmenxtfhmqc47d0lyf0j7ednyjln0u824ue33gvr",
        ]
    }

    fn gen1_change_addresses() -> Vec<&'static str> {
        vec![
            "kaspa:qrqrnyzdwh9ec2q05guzy3vv33f86nvdyw52qwlmk0mewzx3dgdss3pmcd692",
            "kaspa:qqx8jlz0hh0wun5ru4glt9za3v8wj3jn7v3w55a0lyud74ppetqfqny4yhw87",
            "kaspa:qzpa69mrh2nj6xk6gq38vcnzu64necp0jwaxxyusr9xcy5udhu2m7uvql8rnd",
            "kaspa:qqxddf76hr39dc7k7lpdzg065ajtvrhlm5p3edm4gyen0waneryss2c0la85t",
            "kaspa:qps4qh9dtskwvf923yl9utl74r8sdm9h2wv3mftuxcfc2cshwswc6txj0k2kl",
            "kaspa:qrds58d6nw9uz7z93ds4l6x9cgw3rquqzr69dtch6n4d8fxum8c65f7nqmhzx",
            "kaspa:qrajjrpj0krqkww7rymwuwzcd36grjr6688ynvna649q26zukhcq6eqf4jmnx",
            "kaspa:qrumkgz7hlsa748tnzvpztmf6wu9zsgqh6rppw4gzw2mvyq4ccj0y3ms9ju5l",
            "kaspa:qz2g3cj3jcklk4w95djwnm9dffcwg75aqct2pefsxujrldgs08wac99rz70rc",
            "kaspa:qznmzsvk0srfkur8l9pf55st0hnh3x8tmdyskjl9570w99lxsgs7cwrhxap2r",
            "kaspa:qptamza95k7tchmukulldps4kl6wk853dnwa52t4azzm76h588qjufmnu3rn7",
            "kaspa:qqt9h5cjqu9an68cn9k9jc2ywqmqu6kswjzeu09tqulswxkuccaxg6wz45f5r",
            "kaspa:qphr6uy46ad3ca7rerzkx7kkzfzsvfe0xanh4u5mrh538cexs4yjkww0pa4dh",
            "kaspa:qzv3qlh5q4fpy6eu5s4wj080l64del4lvg986z5uh0c3g7wf6n8pvsgm3c9e0",
            "kaspa:qp2dd6y4szgyhcendh7ncxws0qvx8k3s92tg7lvy8eel5npg4pd2ks0ctx4hl",
            "kaspa:qpkqvnkler4rwlpt720unepf3q8cayv0shx0vzydrae7a6u7ryy8zdvnmncyc",
            "kaspa:qr4v33jupxv9h6juqads0znrnw6g7an2ajuzusthnjqujquz66rewtjekhz4l",
            "kaspa:qz5pq2yzpz8ce5avrsa4uzzwrlr5a86rvs74afd6qdm3h649v08nk0qxhrl9n",
            "kaspa:qrajmn035raezl6rcvd0wvnfmdnc0qzwr686ccsrn3z5x8aqnpt8qa0e954jk",
            "kaspa:qrqg7r05nk7syxjh8rdz8wanzmyh8sdts9uexxnnwkq8fplrjammvcnrdggw0",
        ]
    }

    #[tokio::test]
    async fn hd_wallet_gen1() {
        let master_xprv =
            "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";

        let hd_wallet = WalletDerivationManager::from_master_xprv(master_xprv, false, 0, None);
        assert!(hd_wallet.is_ok(), "Could not parse key");
        let hd_wallet = hd_wallet.unwrap();

        let receive_addresses = gen1_receive_addresses();
        let change_addresses = gen1_change_addresses();

        for index in 0..20 {
            let pubkey = hd_wallet.derive_receive_pubkey(index).unwrap();
            let address: String = PubkeyDerivationManager::create_address(&pubkey, Prefix::Mainnet, false).unwrap().into();
            assert_eq!(receive_addresses[index as usize], address, "receive address at {index} failed");
            let pubkey = hd_wallet.derive_change_pubkey(index).unwrap();
            let address: String = PubkeyDerivationManager::create_address(&pubkey, Prefix::Mainnet, false).unwrap().into();
            assert_eq!(change_addresses[index as usize], address, "change address at {index} failed");
        }
    }

    #[tokio::test]
    async fn wallet_from_mnemonic() {
        let mnemonic = "fringe ceiling crater inject pilot travel gas nurse bulb bullet horn segment snack harbor dice laugh vital cigar push couple plastic into slender worry";
        let mnemonic = kaspa_bip32::Mnemonic::new(mnemonic, kaspa_bip32::Language::English).unwrap();
        let xprv = kaspa_bip32::ExtendedPrivateKey::<kaspa_bip32::SecretKey>::new(mnemonic.to_seed("")).unwrap();
        let xprv_str = xprv.to_string(kaspa_bip32::Prefix::KPRV).to_string();
        assert_eq!(
            xprv_str,
            "kprv5y2qurMHCsXYrpeDB395BY2DPKYHUGaCMpFAYRi1cmhwin1bWRyUXVbtTyy54FCGxPnnEvbK9WaiaQgkGS9ngGxmHy1bubZYY6MTokeYP2Q",
            "xprv not matched"
        );

        let wallet = WalletDerivationManager::from_master_xprv(&xprv_str, false, 0, None).unwrap();
        let xpub_str = wallet.to_string(Some(kaspa_bip32::Prefix::KPUB)).to_string();
        assert_eq!(
            xpub_str,
            "kpub2HtoTgsG6e1c7ixJ6JY49otNSzhEKkwnH6bsPHLAXUdYnfEuYw9LnhT7uRzaS4LSeit2rzutV6z8Fs9usdEGKnNe6p1JxfP71mK8rbUfYWo",
            "drived kpub not matched"
        );

        println!("Extended kpub: {}\n", xpub_str);
    }

    #[tokio::test]
    async fn address_test_by_ktrv() {
        let mnemonic = "hunt bitter praise lift buyer topic crane leopard uniform network inquiry over grain pass match crush marine strike doll relax fortune trumpet sunny silk";
        let mnemonic = kaspa_bip32::Mnemonic::new(mnemonic, kaspa_bip32::Language::English).unwrap();
        let xprv = kaspa_bip32::ExtendedPrivateKey::<kaspa_bip32::SecretKey>::new(mnemonic.to_seed("")).unwrap();
        let ktrv_str = xprv.to_string(kaspa_bip32::Prefix::KTRV).to_string();
        assert_eq!(
            ktrv_str,
            "ktrv5himbbCxArFU2CHiEQyVHP1ABS1tA1SY88CwePzGeM8gHfWmkNBXehhKsESH7UwcxpjpDdMNbwtBfyPoZ7W59kYfVnUXKRgv8UguDns2FQb",
            "master ktrv not matched"
        );

        let wallet = WalletDerivationManager::from_master_xprv(&ktrv_str, false, 0, None).unwrap();
        let ktub_str = wallet.to_string(Some(kaspa_bip32::Prefix::KTUB)).to_string();
        assert_eq!(
            ktub_str,
            "ktub23beJLczbxoS4emYHxm5H2rPnXJPGTwjNLAc8JyjHnSFLPMJBj5h3U8oWbn1x1jayZRov6uhvGd4zUGrWH6PkYZMWsykUsQWYqjbLnHrzUE",
            "drived ktub not matched"
        );

        let key = wallet.derive_receive_pubkey(1).unwrap();
        let address = PubkeyDerivationManager::create_address(&key, Prefix::Testnet, false).unwrap().to_string();
        assert_eq!(address, "kaspatest:qrc2959g0pqda53glnfd238cdnmk24zxzkj8n5x83rkktx4h73dkc4ave6wyg")
    }

    #[tokio::test]
    async fn generate_addresses_by_range() {
        let master_xprv =
            "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";

        let hd_wallet = WalletDerivationManager::from_master_xprv(master_xprv, false, 0, None);
        assert!(hd_wallet.is_ok(), "Could not parse key");
        let hd_wallet = hd_wallet.unwrap();
        let pubkeys = hd_wallet.receive_pubkey_manager().derive_pubkey_range(0..20).unwrap();
        let addresses_receive = pubkeys
            .into_iter()
            .map(|k| PubkeyDerivationManager::create_address(&k, Prefix::Mainnet, false).unwrap().to_string())
            .collect::<Vec<String>>();

        let pubkeys = hd_wallet.change_pubkey_manager().derive_pubkey_range(0..20).unwrap();
        let addresses_change = pubkeys
            .into_iter()
            .map(|k| PubkeyDerivationManager::create_address(&k, Prefix::Mainnet, false).unwrap().to_string())
            .collect::<Vec<String>>();
        println!("receive addresses: {addresses_receive:#?}");
        println!("change addresses: {addresses_change:#?}");
        let receive_addresses = gen1_receive_addresses();
        let change_addresses = gen1_change_addresses();
        for index in 0..20 {
            assert_eq!(receive_addresses[index], addresses_receive[index], "receive address at {index} failed");
            assert_eq!(change_addresses[index], addresses_change[index], "change address at {index} failed");
        }
    }

    #[tokio::test]
    async fn generate_kaspatest_addresses() {
        let receive_addresses = [
            "kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd",
            "kaspatest:qzn3qjzf2nzyd3zj303nk4sgv0aae42v3ufutk5xsxckfels57dxjnltw0jwz",
            "kaspatest:qpakxqlesqywgkq7rg4wyhjd93kmw7trkl3gpa3vd5flyt59a43yyn8vu0w8c",
            "kaspatest:qz0skffpert8cav6h2c9nfndmhzzfhvkrjexclmwgjjwt0sutysnwme80mr8t",
            "kaspatest:qrmzemw6sm67svltul0qsk3974ema4auhrja3k68f4sfhxe4mxjwxw752m0ud",
            "kaspatest:qpe4apax5dquy600py9rprmukhq8fqyqv9qu072twkvgse0glhqa75z4a5jc8",
            "kaspatest:qrptdge6ykdq672xqjd4rv2cedwdcz030jngsr2xhaxrn5l8pfhc2ynq7hqh7",
            "kaspatest:qqnys5nyennjkvyl77vwneq5j2vmjss57zerd88ptzaeqhm998smx0vp8yfkm",
            "kaspatest:qztckuvk02885rdazvj9w079qujg5qpxcdnmsvxqx0q8z7l483prk3y5mpscd",
            "kaspatest:qrp53krck4m0x6n0dxs7vzf5mg0x6we8e06xjpmu8xr8p4du6f89kkxtepyd2",
            "kaspatest:qr4l3mahqe0jeeu6c474q5tywz08mudhddgtdneeq46unv0qx0j77htdcm5dc",
            "kaspatest:qzatdsueklx7pkfzanh9u0pwr47sd3a25gfm8wypsevdejhhpj8cks2cwr2yk",
            "kaspatest:qqk3g5l6ymdkjfmzezx4zrv9fhr5rh0d8tm07udkqxq79n6t60tzus0m9sd3v",
            "kaspatest:qqasa6d590u6875hsese68fa9f8mnedzesn2udehp0s73ggt5cklwtwl220gy",
            "kaspatest:qpuzq5jc757uxue9fradme33jd6egxr9fdznd8qysqcc5xy8k7alqq5w6zkjh",
            "kaspatest:qqygznwmkl56vprrnvyvnta9qql43yv52m3qz2462vxskn32axl0xe746l7hp",
            "kaspatest:qqk974yml6uuustenwu57hn8n7d202luvn4dum0txvzjgg60g2jzsh44nc8vj",
            "kaspatest:qpxqat995cxnjla8nm0dwnneesqnk5enc6hqrua7jztels0eqjg8v3af29pl2",
            "kaspatest:qpyzkjs2a6k8ljx2qt4pwscj6jccr6k7pmru9k7r2t25teajjuzazlyszlz7a",
            "kaspatest:qzf5mxtvk8wgp8gr3dcj3dkzdu6w4dgpvp2f0gm9pepv9vazxrhy5lc0lgqj0",
        ];

        let master_xprv =
            "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";

        let hd_wallet = WalletDerivationManager::from_master_xprv(master_xprv, false, 0, None);
        assert!(hd_wallet.is_ok(), "Could not parse key");
        let hd_wallet = hd_wallet.unwrap();

        //let mut receive_addresses = vec![]; //gen1_receive_addresses();
        //let change_addresses = gen1_change_addresses();

        for index in 0..20 {
            let key = hd_wallet.derive_receive_pubkey(index).unwrap();
            //let address = Address::new(Prefix::Testnet, kaspa_addresses::Version::PubKey, key.to_bytes());
            let address = PubkeyDerivationManager::create_address(&key, Prefix::Testnet, false).unwrap();
            //receive_addresses.push(String::from(address));
            assert_eq!(receive_addresses[index as usize], address.to_string(), "receive address at {index} failed");
            //let address: String = hd_wallet.derive_change_address(index).await.unwrap().into();
            //assert_eq!(change_addresses[index as usize], address, "change address at {index} failed");
        }

        println!("receive_addresses: {receive_addresses:#?}");
    }
}
