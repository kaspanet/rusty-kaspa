use crate::derivation::gen1::WalletDerivationManager;
use crate::derivation::traits::WalletDerivationManagerTrait;
use crate::imports::*;

///
/// Helper class to generate public keys from an extended public key (XPub)
/// that has been derived up to the co-signer index.
///
/// Please note that in Kaspa master public keys use `kpub` prefix.
///
/// @see {@link PrivateKeyGenerator}, {@link XPub}, {@link XPrv}, {@link Mnemonic}
/// @category Wallet SDK
///
#[wasm_bindgen]
pub struct PublicKeyGenerator {
    hd_wallet: WalletDerivationManager,
}
#[wasm_bindgen]
impl PublicKeyGenerator {
    #[wasm_bindgen(js_name=fromXPub)]
    pub fn from_xpub(kpub: XPubT, cosigner_index: Option<u32>) -> Result<PublicKeyGenerator> {
        let xpub = ExtendedPublicKey::<secp256k1::PublicKey>::try_from(kpub)?;
        let hd_wallet = WalletDerivationManager::from_extended_public_key(xpub, cosigner_index)?;
        Ok(Self { hd_wallet })
    }

    #[wasm_bindgen(js_name=fromMasterXPrv)]
    pub fn from_master_xprv(
        xprv: XPrvT,
        is_multisig: bool,
        account_index: u64,
        cosigner_index: Option<u32>,
    ) -> Result<PublicKeyGenerator> {
        let xprv = ExtendedPrivateKey::<SecretKey>::try_from(xprv)?;
        let path = WalletDerivationManager::build_derivate_path(is_multisig, account_index, None, None)?;
        let xprv = xprv.derive_path(path)?;
        let xpub = xprv.public_key();
        let hd_wallet = WalletDerivationManager::from_extended_public_key(xpub, cosigner_index)?;
        Ok(Self { hd_wallet })
    }

    #[wasm_bindgen(js_name=receivePubkeys)]
    pub fn receive_pubkeys(&self, mut start: u32, mut end: u32) -> Result<PublicKeyArrayT> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.receive_pubkey_manager().derive_pubkey_range(start..end)?;
        Ok(Array::from_iter(pubkeys.into_iter().map(|pk| JsValue::from(PublicKey::from(pk)))).unchecked_into())
    }

    #[wasm_bindgen(js_name=receivePubkey)]
    pub fn receive_pubkey(&self, index: u32) -> Result<PublicKey> {
        Ok(self.hd_wallet.receive_pubkey_manager().derive_pubkey(index)?.into())
    }

    #[wasm_bindgen(js_name=changePubkeys)]
    pub fn change_pubkeys(&self, mut start: u32, mut end: u32) -> Result<PublicKeyArrayT> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end)?;
        Ok(Array::from_iter(pubkeys.into_iter().map(|pk| JsValue::from(PublicKey::from(pk)))).unchecked_into())
    }

    #[wasm_bindgen(js_name=changePubkey)]
    pub fn change_pubkey(&self, index: u32) -> Result<PublicKey> {
        Ok(self.hd_wallet.change_pubkey_manager().derive_pubkey(index)?.into())
    }

    #[wasm_bindgen(js_name=toString)]
    pub fn to_string(&self) -> Result<String> {
        Ok(self.hd_wallet.to_string(None).to_string())
    }
}
