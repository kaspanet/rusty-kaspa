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

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::{PubkeyDerivationManagerV0, WalletDerivationManagerV0, WalletDerivationManagerTrait};
    use kaspa_addresses::Prefix;

    fn gen0_receive_addresses() -> Vec<&'static str> {
        vec![
            "kaspa:qqnklfz9safc78p30y5c9q6p2rvxhj35uhnh96uunklak0tjn2x5w5jqzqtwp",
            "kaspa:qrd9efkvg3pg34sgp6ztwyv3r569qlc43wa5w8nfs302532dzj47knu04aftm",
            "kaspa:qq9k5qju48zv4wuw6kjxdktyhm602enshpjzhp0lssdm73n7tl7l2fgc4utt4",
            "kaspa:qprpml6ytf4g85tgfhz63vks3hxq5mmc3ezxg5kc2aq3f7pmzedxx6a4h8j0f",
            "kaspa:qq7dzqep3elaf0hrqjg4t265px8k2eh2u4lmt78w4ph022gze2ahu64cg5tqa",
            "kaspa:qrx0uzsnagrzw259amacvae8lrlx2kl2h4dy8lg9p4dze2e5zkn0w8facwwnh",
            "kaspa:qr86w2yky258lrqxfc3w55hua6vsf6rshs3jq20ka00pvze34umek35m9ealc",
            "kaspa:qq6gaad4ul2akwg3dz4jlqvmy3vjtkvdmfsfx6gxs76xafh2drwyv5dm54xaz",
            "kaspa:qq9x43w57fg3l6jpyl9ytqf5k2czxqmtttecwfw6nu657hcsuf8zjfveld7yj",
            "kaspa:qr9pzwfce8va3c23m2lwc3up7xl2ngpqjwscs5wwu02nc0wlwgamjuma2j7qs",
            "kaspa:qr3spcpku68mk9mjcq5qfk4at47aawxl2gz4kzndvu5jn4vzz79djffeqhjnl",
            "kaspa:qp4v6d6lyn8k025fkal869sh6w7csw85gj930u9r5ml7anncqz6s7u7fy7fpf",
            "kaspa:qzuas3nekcyl3uv6p8y5jrstchfweue0tpryttn6v0k4vc305rreje5tvf0t6",
            "kaspa:qpy00e8t4zd5ju8069zwsml2m7z3t607s87k0c66ud338ge682qwqlv7xj64c",
            "kaspa:qrs04ra3yl33ejhx6dneqhm29ztdgmwrxw7ugatmecqqm9x5xvmrx499qn2mh",
            "kaspa:qq5qertse2y6p7vpjcef59ezuvhtdu028ucvvsn90htxvxycavreg506hx2r4",
            "kaspa:qrv30p7gatspj5x4u6drdux2ns5k08qxa3jmvh64ffxcqnxz925gsl203p008",
            "kaspa:qqfupvd2mm6rwswkxs0zp9lzttn690grhjx922wtpt7gfnsjdhk0zhhp07mhr",
            "kaspa:qq2un0yhn4npc0rt2yjkp4aepz4j2rkryp59xlp6cvh0l5rqsndewr9zth6h8",
            "kaspa:qzams4ymck03wfqj4xzvj39ufxl080h4jp32wa8hna2hua9kj6t6cldumhm2p",
            "kaspa:qrzngzau800s9esxr5kq5kytp5l2enttf8xaag2pfz8s0e4k535767arhku9w",
            "kaspa:qpkpfagtqaxgzp8ngd3mwqf5n3pnqprp0dljukq0srlv4h0z09ckx9wfl9qsg",
            "kaspa:qqgxfpgzthxq4t2grv7jcshc0r9szsqttffh5cq7500lnxmpagvr6duvmfhqe",
            "kaspa:qq7m66z6dgdvqzmtg4zllh46l978cpud33zx7kcgcnf359glz0ucjw0tdzg6t",
            "kaspa:qrf6gzyhlfmmmd7yt7h45rrt37cpuzuyztyudwg3gfl3lpqenvk9jh4eptada",
            "kaspa:qznrj6r0yw3e3fjmy2ffa3wmkzcjaftljc9j360dwum8hpay3jdgjkxwe5g0w",
            "kaspa:qrh7p9x2kh0ps9crvgrths55rannuawc2lppzdn28na0yu9dmw5nkzhfnz4ps",
            "kaspa:qqd7g3skxjp7desmz99wy762uk59q8hqxxgm6tcgm0kw49x9d0l82la2evv05",
            "kaspa:qzxamdddkg429xexzd39dzlvpnwpvt0202a3hhvstdct49gv9yzx57xxazlfn",
            "kaspa:qzc8w4t4jxpwntqnm6fyl80c2e74mrunzk4l0yuuq6cmm355mq2e2350k56kw",
            "kaspa:qpeumknudvt6vpvkv9rahptrxu3wdjte62cz4nh33qc65gjvc6xuznhvuhhwz",
            "kaspa:qp7cdnnlcfa8r0fy7yduuhsexyagqpp9cqd8efj9v07r43fpnmg6qmuxd4d9g",
            "kaspa:qp7wxlf0hec690n6at259qww600sqakft8dnn2ujr6a7sk35snh5u2jjc0lnu",
            "kaspa:qzpczl9smaz7axyqmnkvd0694z7jpfrcgl9lka0h0t8fqy8efzqhv7nv86dad",
            "kaspa:qpfxwpv26rr7zydqdpmxtevch0qpgaldypjrctnhcrt5lccy6d8dupdqye970",
            "kaspa:qzj4vc7yw663v3akdldfcp6r2g69pej5kdc0jusp349yq57yk7xrk69ygc04h",
            "kaspa:qq2dha0feemswy9twtk9fys7tmd9gus8nnl38kqt86qdvq34rvymj27t6za8u",
            "kaspa:qpsx92u08vse22yhm4w0s56jf8drxa9al208a7dycl88ppc22eyjuxn7qzmkh",
            "kaspa:qptr20fsz9lpklpzyynnttjwf848cw2s8mqyzddkmyr4q4yclhm2z2az8l2tn",
            "kaspa:qzecr9vqwxas7d3rlt9s6dt5ku9xacwqvlsxjdkn3n3sa7q7kttqwuk380c9j",
            "kaspa:qq004kxhnwh39z3du9hu5yednllspuu33x3gz5zlvj6t8kac9urnv7alw6ava",
            "kaspa:qq3e77faqa2auktf3jq7lj4vnaf70p856vlxg28dat2wcz22tjttkxqvkrhky",
            "kaspa:qr83hneey4c9846xxn2uvszx42jyx20fpnyrpamy8cy8dhdpljq4xf85l3679",
            "kaspa:qz7wphuhuvx9ac2mp5td50dq25mzpkca9r2d35n2ek929e6qa72rq6hvtr95l",
            "kaspa:qrgsrdp3ag630cpjfzrvfa9gd4dafnrpmf2qwk4cy5mum7tk0ph4cd84kpxk8",
            "kaspa:qr4dhfm6cpp50q0lsg2drzv0nj5n4r57adfpxkwss3hf53wau2stumm2g89y0",
            "kaspa:qzrc652du8tapgrv7rfkmykqzeep8jrgsjeynypldq9mfn5phcyxk3xl8cfpn",
            "kaspa:qzauugr73lu4rjryhqmczk699775yshltpdxsxxd0str7jkttyxgwjr654e2c",
            "kaspa:qq2a7m6pjgm85erx3nhhex9uqgsjtkup09t2tukappztyz4f9ykas32uqc305",
            "kaspa:qrnjfugy6c9eg5g60fhfnh36069pzpz7z0t9nuzrg5whd6e6ut2ns98l3y5ra",
            "kaspa:qrhnvydk5dt2q9vk2f37vf848zztq4ex06rvwq5x3tymle73q08wzkqfwtatc",
            "kaspa:qrchv5j6sqmwpk9fumd2jz6na26ulxgcy7uwjlg95nur6mukhdcmvvxug5nkr",
            "kaspa:qq26pgvl5f4x3rdrf5jw9zn2e02n8xae4yvp7m4mfqf0n0mldjthjshc5g9my",
            "kaspa:qrmdeltxu3gzjgfpehucyufsm08fm924akwm3x05uzp8m45tr0raskkdlgelm",
            "kaspa:qrvzeg6qqqx6lvv0d3kt22ghj8lr2jvfpaypp8hgyyn75a9qmjqvypxrnwkq8",
            "kaspa:qqx5krm2a3ulccu8g0wn42lvernz6h42s7rk9yxd3t7xt062jvslwaj44gz2g",
            "kaspa:qql4warf635653r050ppwk9lm8vln2wdwucjnhljxtqnxk2x4axfgm4g4hzl5",
            "kaspa:qqgrtx4nuhjavpwwrfsa7akg6fcna7dmjtpgc69f6ysg8vzjrmwwsjr8j9xpr",
            "kaspa:qrny80e7zurf9sq9pzcesafyat030zkqnt4w02aa9xl8xvh9w0r867ga8cp7n",
            "kaspa:qp0yve4h89udt5rvpzwf3qrecdcscdgfq420eh2d9v43d96t0lwkw07y6jdwu",
            "kaspa:qrlx73us8hrfe2g78uw84aqerz9ea889rwc3e7pezvwv7rakcr3mk2wm9xm44",
            "kaspa:qrpjp0m0x2708vazdajlfct6e2pnxc2xk5kndz7glg2akug2fl48j8qa2ztp8",
            "kaspa:qr82t672mwqrqym3p8s00aevqkp67x2hdrhj7079shsdxz4kecf3j45vf3q22",
            "kaspa:qzqkv08jvktzyxl9829d0lgg2h2quurq0gr263atmpj9zevaj2ze5z3pmdcg0",
            "kaspa:qz0cg9990rddlscth27syxcgr6x6xxkyjjyn6jn9lgd7r3pd6064cmkdl06kt",
            "kaspa:qza4cgmzy4x3ztlhmaf3v3fnr8ghazd7lengrpdrtfsspyaz0p7yssp2cjtkk",
            "kaspa:qp44w4lq42ck4zm9r0gga8uz6ghzug3jcd4ju9cmnn3ld9sz3r3sk4hjtkf4u",
            "kaspa:qqa6k29l06ht6vvtspfgq7qvflyum3ya4k98rnglhpuvnsus3jynjwxmv8d5d",
            "kaspa:qz6rmppc4h9zkzv8x79e4dflt5pk0vfr0fnv86qfq4j7lu2mluex7zpvv42cm",
            "kaspa:qqlzdmud9mwfgsmy7zk8ut0p0wvxtrllzt26hv6jffjdy224jartw0p42847n",
            "kaspa:qpvf0wx35uwda732xpgu7fakh37ucdudm335msw4f4aw7fuv3unzx7hnypsv0",
            "kaspa:qzhafa8n9st86gxk07rehpy8fdghy669l8sy57l3fae97r6yc6hlxz4rcss7g",
            "kaspa:qr36fmpfggppn6ch9u5rwflhy5tpgyfhfvtmkglln089f3g38j4tcum4tv3z4",
            "kaspa:qz8r3qrdzfkfp9raq3etxhg5sl6zwuvmpggnprhlhch5ksapj37ty6slewydq",
            "kaspa:qrct5xhxtp87qn3hjnx9e5eatlkez3sw70zywm5n3a8vvzuhqtdez5uhd5ne2",
            "kaspa:qr57llq4lhrd0cf58553najxj4fd4kh4m69tp2s8dlmqh2353sm0v9fn5hg6e",
            "kaspa:qpqqn25lhhyhz9aflteadkzxrvhy390rpjlmcauf5ry5feyvawff2qllms89r",
            "kaspa:qz00pye8ezdsm6h9j6840dzv2cgv8qkrd8a77efl2ealv82vu4l65cj4hvtlc",
            "kaspa:qq2z5vfeqpcvh2f0x8atn67jf6en4vzqhu0ahd9w0fr8ngzgc2fl2mrmyjkwu",
            "kaspa:qz62rs7guer4lyahu5j9xsrn38mcnmnshjl984v5sq8ldtz6m48tqatljzfh5",
            "kaspa:qzmsd5k3h8ztc4ulp0rgnz7httxy7tre6quswrp60xh9emxmw8lvkvf8fys3s",
            "kaspa:qz4patdle0j4q8cg93fs9qkk2uu8tm42jse0x5nn2ssrqsphlptfxp67st0vv",
            "kaspa:qpkzst9yfzcvdfdymkdt69gt7rm3r2ztcjrarl0ss09jcgxzpjvkxz28m25kk",
            "kaspa:qrksn3kunxwkpfudhdwwjhpvsuklz2eq684ghf087zsnvheywpxfvgysnfq0g",
            "kaspa:qzzxrs6wkqnfpyk4gnsn9tajl8rrw2tznecu7uxp62emgmc62u4qsk5udslls",
            "kaspa:qrd26p83evu7remt400h60r370q05y9y3t2eygw0a8ya4n6sp4wacpsefkuyg",
            "kaspa:qzvw3r65mhxa5ekgwdnazlhdqhmxazacht80s2yh9fuw2nxwy23a5rryr8nnr",
            "kaspa:qptu8eegz7y050qxq32ece5sydpdgage07ussm8vuaged9anl62qsq2fwtc6t",
            "kaspa:qza9y7xmw3s8ms63pdc94al4xnllw95kzlegnsuk0zyw2hvzx5e557mrlr0jp",
            "kaspa:qq75ps5c4de6jrg3vq4nz8gtvsflh79pulwf7avcrs2s0z9z6psw6s6muwadp",
            "kaspa:qp3085yvwxj2v52u7dv5v5w63k9vlf677zlya2krj5jpp69w2e3gk6ktuv8ql",
            "kaspa:qqjaqpnzxfqwkuuyjd7qvulgx804uta9wtkdntphc36kc3nj9xgg29sft82sl",
            "kaspa:qprptscwd4tyhjh2eyc9ve5paxcap7k88mz84q0sk36ajhlje3a5kdf7qt56w",
            "kaspa:qq7mf20qh9g4rtf4h76wepcpjem0x7jq39qy875ra2jk4m8gzc7452m2tnz96",
            "kaspa:qpydw5azt092uhwscnn96pflcnyn5e264f2lxmxhufj27cptzz8evw5hghynp",
            "kaspa:qzm375sk4xgacy0smneq9kuwza8g2l664cer3vlmv7mvwg0m5nw8u0scdv84q",
            "kaspa:qrw8r594tdzy026rqpe4pa830qxcsjqhzlv7p438x939292kvqaxvsgfe56m8",
            "kaspa:qppe5llh7m75z084xrjt0y5thfss5u6srl945ln2r4039ce937pwwz5lanqxy",
            "kaspa:qqw55sj3x3tvvpy0ufk0rarz0zxnmj2avhukvswgy4h0d6cxxmy0kqfr8lsnd",
            "kaspa:qzrmdyudtf7uv7g5f5pnv0x93r3c85084rgd8mhxgync66rkpjml26a28tdjl"
        ]
    }

    fn gen0_change_addresses() -> Vec<&'static str> {
        vec![
            "kaspa:qrp03wulr8z7cnr3lmwhpeuv5arthvnaydafgay8y3fg35fazclpc6zngq6zh",
            "kaspa:qpyum9jfp5ryf0wt9a36cpvp0tnj54kfnuqxjyad6eyn59qtg0cn606fkklpu",
            "kaspa:qp8p7vy9gtt6r5e77zaelgag68dvdf8kw4hts0mtmrcxm28sgjqdqvrtmua56",
            "kaspa:qzsyzlp0xega2u82s5l235lschekxkpexju9jsrqscak2393wjkdcnltaa0et",
            "kaspa:qpxvpdfpr5jxlz3szrhdc8ggh33asyvg4w9lgvc207ju8zflmxsmgnt3fqdq6",
            "kaspa:qz28qjteugexrat7c437hzv2wky5dwve862r2ahjuz8ry0m3jhd9z72v4h8w9",
            "kaspa:qz8cus3d2l4l4g3um93cy9nccmquvq62st2aan3xnet88cakhtljuk69seejg",
            "kaspa:qzczlu9crsn9f5n74sx3hnjv2aag83asrndc4crzg2eazngzlt0wq90zqsfm7",
            "kaspa:qqemqezzrgg99jp0tr8egwgnalqwma4z7jdnxjqqlyp6da0yktg5x9qfe9mwx",
            "kaspa:qr0nfhyhqx6lt95lr0nf59lgskjqlsnq4tk4uwlxejxzj63f2g2acs7c3nvtv",
            "kaspa:qqp0s3dacp46fvcaq5v2zl43smk2apzslawjqml6fhudfczp5d9n2p0s34t0s",
            "kaspa:qzac4rjzem4rvzr6kt2yjlq7whawzj9ra9calpw0euf507fdwuskq567ej2yt",
            "kaspa:qrupjagxeqqzahlxtpraj5u4fd7x3p6l97npplge87pgeywkju47zqcdua4yg",
            "kaspa:qz208ms8heafvt90d28cpm3x7qvav87e3a2hgcz0e5t3d84xmlvcqqx9wg9pg",
            "kaspa:qq5357axc5ag8hzytf66p3fzw8d578h7xyfm4x4cpr3lp0wallglk6sfmklua",
            "kaspa:qzsjhgefa98e4fsk58znu03mwzw7ymj7t4392l69kp0pgml2ymqm63njk5vxf",
            "kaspa:qplnwp0lxzwykmxrqphu62drmem2d09kfzplfek8z7cwt4s3vkkakvek89fhv",
            "kaspa:qr4cm8smzgt8gzg33csv9mrsnvj9809ffun89cqsw65q3a37vmqx5ng67x8h4",
            "kaspa:qpj0d7nznxp3nn2kyqsvm0ns38hzdk7dhj8g90cnrv9jda8xw5q2y8v8q7yh3",
            "kaspa:qp4qt5cjrq73nuatnlwnk90lz5kqpd4mpqm53x7h3lpu74phz6zm5g9qmy00m",
            "kaspa:qzjrlcwkl2mssucyyemnxs95ezruv04m8yyek65fyxzavntm9dxtkva30sv3u",
            "kaspa:qz24dfwl08naydszahrppkfmkp2ztsh5frylgwr0wqvjqwnuscvmwg2u0raml",
            "kaspa:qqy8pv5sv9quqce26fhn0lygjmuzrprlt90qz6d4k2afg0uaefptgva6l52ee",
            "kaspa:qpmqpmnwhqv7ng24dh6mj6zqm0zptkgv0fvetcgqgv8vdukk3y59ycm40t66l",
            "kaspa:qrumw263pj7gw8jqye7kd58gqq6lgnv2fjvevuf55wptvqp0r5ryj4f29upt3",
            "kaspa:qzv60vtkmaaxgp4kfj86yjxt9w03qgxma5rmfsgwupeguxhgtnq0ytrnfj2gp",
            "kaspa:qzyn8xpvuh8vfsp0zd8rc3990dgwlhrukt26xdqt0zcu5mm8jsjcyf5x95cwl",
            "kaspa:qzrvh8zyclunxu3dfuqyp5yv853ejeqqkfp2gcyyyq3mju5ame5xsv3g857ky",
            "kaspa:qpfkj0emekeqvsc925cnna9mt8zhtazfwcjfjd3kss4f8fvensppz24wckvcx",
            "kaspa:qq2hv6nhxegvex8vqaun6cjpmgu6lelf6l6mfz4565zn3qjwjlu0kmlfutzgr",
            "kaspa:qrnclejggdsg4ds8fxmgcmn22sy2w5704c6d9smug7ydyd65grzk23ty4jzyv",
            "kaspa:qz74fxk35jc0g8s4u76uxcdahahhumu4ttzfmcu94vqkymla33lmkykxwfqdn",
            "kaspa:qpmpe7s45qmx3gzehuhh8nra9x5sk3s5hdr6e7jlyhtrjq6zhtt6cmp8r6hs3",
            "kaspa:qzz4v2h3s2y7dvpsy6pt0urrjx3rw658t25g6uj9jfvx4f8vwhetc604fe5l9",
            "kaspa:qqz06pskea8ktjwfn90y46l366cxlt8hw844ry5xz0cwv5gflyn9vasks28s3",
            "kaspa:qzja0zah9ctrlg2fs6e87lac2zal8kngn77njncm6kv6kxmcl5cwkjq4c62mq",
            "kaspa:qzue8jx7h2edm3rjtk4fcjl9qmq59wrhg6ql2r4ru5dmc4723lq0zf4jjsxvk",
            "kaspa:qp0yeh6savdtyglh9ete3qpshtdgmv2j2yaw70suhthh9aklhg227erlpvdrc",
            "kaspa:qrzp9ttdmpc94gjxarq97y3stguw9u72ze02hd7nl30whydz44uhudmk0xsxh",
            "kaspa:qzyfayq2tu5q6t5azlr89ptq0mcplv8m4zdmmtrve26cevydfkn26qgna8acl",
            "kaspa:qr6w0un2pde7sm29793srwqwq5p2vqhq8q39l4g6dhx2x9p0wg8ly673pm4na",
            "kaspa:qpp2qwmk7v3tlfxcg0gvd960f90t6fx42wtfszjnh5m8y8j5cygtwkk62v3wv",
            "kaspa:qqp6p2rmml9anfs09wqsu2e6j4mjmndczju7psnkm9hh84j42k9cwm3lcgxwn",
            "kaspa:qz3ze0g3n9xe3dzs98h5xf3wfk3wlzz3h2zg3cppvaeq4xcamhpe7xfa08ek5",
            "kaspa:qqgjuzgapalk9k8zm0vp0nndtvfk043drm8n0hk05px40tv8eaxejzne0ylgy",
            "kaspa:qraklk33dys3js328admu2vpdj37zhc05fnr983c3new2etks3fz5078yw8ng",
            "kaspa:qzm6tvhluq0cvy9cuhwuaye05wch62gcnf0nsmjpw2gur3a7pyrhgcdga35fp",
            "kaspa:qqexh4wwjuncvmm7cycyrpjwnuknvk50f9zmahjt0m2tmex5zx02uarmtdkv8",
            "kaspa:qredxcud88qfq39zltc3sc6425g6d36v66sx6rv236f42g65tc4yx7nzsmnak",
            "kaspa:qpnuv59xjnj49quayyyn7n0zyunu9u4q7650s8w85k9ef6379ermkrgyl92mw",
            "kaspa:qpfvr7qvsy0hxhy2hg2jtm5kr8vnvr49d7yn9wpcymf8pjeekpnq2ql34s7pc",
            "kaspa:qph0vh0wtu59yklxpgthfr7upnya28duzkkgr5l43urj6qvy65stk77nutmd8",
            "kaspa:qq9dmujd78f4et7nq3qdquuq4gth29r2ln2qt054qyy5rpyjfymyuj4y6nq4r",
            "kaspa:qpdt4tz7yc2atpdu5g9kkx0v4lsjsd6jdw3euc25rv6pmn5nvakxkag8c5tde",
            "kaspa:qz9yfegr2aw2skxf4lsdw4v4jruemapw6ehkv5x0yp0985kz5p6wc4sy2p7m2",
            "kaspa:qr9guyx4j9an7rnml36vfwpurn8nut3g4534j5wvkv4aqvdsn05mv7ty3crnm",
            "kaspa:qz7a4mu29gf8ly68s34mpe53s8jd5gxzrmu8vqjre44rdfvhnlpl678napd6d",
            "kaspa:qry4n3pu0f293n7r03k5fty0eksdhnn869vyqcr8td8stcn6l4ql7ew0ldsnq",
            "kaspa:qp5tw4rpvkezcvpcdz8pwln04fxhawekuyfvhrcyjcljpcdkctmucfru40xtl",
            "kaspa:qpkwrwgmh6zh5jfw2stleumkdxjnj4uyxxems3ucy2rk4z7mrnpjyrx68qz6j",
            "kaspa:qzzfgs3lh80avxx7nc0kp7k8esntaaegy63er6vf8vwuxmw3z42wc8ldgjh7h",
            "kaspa:qrakpce50ps6zfjhrux5y99kf75z936rmg20h3tryjht4g5kldwmut4jwg49v",
            "kaspa:qzgay26sfqzmnjtzhtase4sy9ucddfyey3x335z7kmpqlkh4h3laxtl5mlsq9",
            "kaspa:qzsjnxw8ezs7yjzxgy3900548ufz27yjh2g53n8se3qth3spn78jzcgh8luh8",
            "kaspa:qrcngzqx23q82rtuu47ytr68n5974mlczhz485hk93vmhe3a4lq4x85fp2sx7",
            "kaspa:qpncnvnxen0hxn75d6aheel7r6apeylggmyt90a5aapy5aynj8ypgny0nva6z",
            "kaspa:qrg582jtu2vdqym7mysc9ngrhm3jhysswlrf8q8cjadcm34ckeyngqe8vvym5",
            "kaspa:qzrjslkurxallygypt5avdfxzl4ee889fd363t7xdnqyp7lzl4ehxzscy32pz",
            "kaspa:qrr9p4m6wj9nzw6calhk3gus7j2e7q2w88v9swr4hghmxkutqyvfxcksamrkp",
            "kaspa:qzj7clnh0zz7la55yxawsr4lt00av5fkxtel74gfpm96cak49lgdz6vev8hwz",
            "kaspa:qzpnspfucuefnfmxlhuswh5e92lzv9wp7kn25g95xx5nnsvzmyygchet9sz95",
            "kaspa:qrtw6fz7wt73zvyp36vcajfk9sajgl8jxpzxamv56un5lpp9wwunsd6s2vzy0",
            "kaspa:qpq3n27p3nhn3x22jjawchvpl80x6a63faujp0xt6uyx04plcetwu2jga8407",
            "kaspa:qq7de2y9ed6cq5cysd2w897l682s4mtve92s2l075nr8fq3xq2k42xtpe0xaz",
            "kaspa:qp8ccwx0sfscktvz4pus2gh9zckyswgdhw9npxq85wx4ekcwhxv3yezkfcwx2",
            "kaspa:qpfmre8d6nru9v6lfn3u643aa2jq9gjs89pe499cna8fpsr0h39868e4vnn73",
            "kaspa:qrvnmyphgyqpenm9xe0qqsfdul2xf9ylmkxvxjxkrvq3rfn6sy895s8jgaefa",
            "kaspa:qzf43vv4ytjzy46srr6amxr0yhyh392hygq089m932e3w9su602fqxga35a7t",
            "kaspa:qz7kme8jqvvx7kamr7r2kdhwt4jwaq0glqdh7rh6jneymv3nhz0hu2tpgd9ld",
            "kaspa:qzrgx4h0w3jzy39ade57lynljn2kcqay4ralmnjvr7anz3vzg3yaqj0h5k4pd",
            "kaspa:qrevlha74yuz6sltmh9w0qjmj3gt3xrt2s7z4e8f4narf355tuq55pj5uaefs",
            "kaspa:qq2c6p62l2z43sakuee5tspr7ctpfv370kuq2fmmqk76ya4wcdmmyk4z06pks",
            "kaspa:qpcf9yfxzss3cjh70n3wau3uhq844txz6pw2sd507lnkygv06xtm5yajwqdyk",
            "kaspa:qzjm2uk405lzzmyn4ad9x6736qy4gxw84vkdpylrjmegzv0e3nqrkna5cnwly",
            "kaspa:qz4rfm4drdvj9yqz4pzx68zjq5zmgueclwmzd9femj4rm0x9n5m8qyk3cfxsf",
            "kaspa:qr8h52caava83pk77nraxaea7g2yvumjjp29f82lyh2qcdx47ngcy40estyl7",
            "kaspa:qp2uxlg9mtehpj3cx83stwq9tjv2tu3cm8dcf62xzvy5t75jputccclm89r4d",
            "kaspa:qr9kp5p0k3mx8n8qwkfckppm3q3c4pup347n2qygfq80hxsljtu2srm34g4h8",
            "kaspa:qrlpxflqrspyn8rjk93lst8tja0xt6jzmv7msmzugpjn5t7j7w3c2kdxljtxc",
            "kaspa:qzc7rk8gm7k0z27j9gjag54e2k46tghscryhwe549g24340sha4kuv2kv4dul",
            "kaspa:qrr7v7zu9qpleenec5s5rl7frxwemrewtehzlm47pa8lkqqgy3nw6eq8v0sv5",
            "kaspa:qzu5ent4t0f4fzz0muf5qqmrspqn4re35w77mlujzfsnjtpglhg8segj8m6n3",
            "kaspa:qznp2z9dn4dfapk478mv8cpr5zh8qj69wv2mpydfzw7weh9aacjvs7ryvtfgz",
            "kaspa:qqd7xdpywvlmrc86weqay2z3ve85f25tdfffn6phd47shmtsrrzzw6ulp2vhr",
            "kaspa:qrl4rhex484u46n8y2u9jhf24qefp4ua5hyfechz78p4hl64t648z5ln07h7j",
            "kaspa:qzhmxv8p8gsn3vnf8xqp2ashcvc39a54fpnlwgztcw4wg0g7wuv8c5w6d6pmg",
            "kaspa:qpuz7tpwy49dnjc8udfsm9m65pkv80ey8x722wyaq9ehjjmfywx3gm5sd380m",
            "kaspa:qpgtjsa4f3nnkt62ukyq2eu83w0u7fap906txwajqf5t5uxt9tqmjrk0n9hzy",
            "kaspa:qzlp093qcsspd0nzs8x9v6kxuy2x938hhpn3jw9l8s6lafykwe8nxpqe4e59w",
            "kaspa:qzlv8cya2gej9y2szg2zj9krrgdwfxr8250apcz7r72rhmk0lv9nk7rn8akju"
        ]
    }


    #[tokio::test]
    async fn hd_wallet_gen0() {
        let master_xprv =
            "xprv9s21ZrQH143K3knsajkUfEx2ZVqX9iGm188iNqYL32yMVuMEFmNHudgmYmdU4NaNNKisDaGwV1kSGAagNyyGTTCpe1ysw6so31sx3PUCDCt";

        let hd_wallet = WalletDerivationManagerV0::from_master_xprv(master_xprv, false, 0, None).await;
        assert!(hd_wallet.is_ok(), "Could not parse key");
        let hd_wallet = hd_wallet.unwrap();

        let receive_addresses = gen0_receive_addresses();
        let change_addresses = gen0_change_addresses();

        for index in 0..100 {
            let pubkey = hd_wallet.derive_receive_pubkey(index).await.unwrap();
            let address: String = PubkeyDerivationManagerV0::create_address(&pubkey, Prefix::Mainnet, false).unwrap().into();
            assert_eq!(receive_addresses[index as usize], address, "receive address at {index} failed");
            let pubkey = hd_wallet.derive_change_pubkey(index).await.unwrap();
            let address: String = PubkeyDerivationManagerV0::create_address(&pubkey, Prefix::Mainnet, false).unwrap().into();
            assert_eq!(change_addresses[index as usize], address, "change address at {index} failed");
        }
    }
}
