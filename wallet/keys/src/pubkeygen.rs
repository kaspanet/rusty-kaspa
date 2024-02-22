use crate::derivation::gen1::WalletDerivationManager;
use crate::derivation::traits::WalletDerivationManagerTrait;
use crate::imports::*;
use workflow_wasm::serde::to_value;

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
    pub async fn from_xpub(kpub: &str, cosigner_index: Option<u32>) -> Result<PublicKeyGenerator> {
        let xpub = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(kpub)?;
        let hd_wallet = WalletDerivationManager::from_extended_public_key(xpub, cosigner_index)?;
        Ok(Self { hd_wallet })
    }

    #[wasm_bindgen(js_name=fromMasterXPrv)]
    pub async fn from_master_xprv(
        xprv: &str,
        is_multisig: bool,
        account_index: u64,
        cosigner_index: Option<u32>,
    ) -> Result<PublicKeyGenerator> {
        let xprv = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?;
        let path = WalletDerivationManager::build_derivate_path(is_multisig, account_index, None, None)?;
        let xprv = xprv.derive_path(path)?;
        let xpub = xprv.public_key();
        let hd_wallet = WalletDerivationManager::from_extended_public_key(xpub, cosigner_index)?;
        Ok(Self { hd_wallet })
    }

    #[wasm_bindgen(js_name=receivePubkeys)]
    pub async fn receive_pubkeys(&self, mut start: u32, mut end: u32) -> Result<StringArray> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.receive_pubkey_manager().derive_pubkey_range(start..end)?;
        let pubkeys = to_value(&pubkeys)?;
        Ok(pubkeys.into())
    }

    #[wasm_bindgen(js_name=changePubkeys)]
    pub async fn change_pubkeys(&self, mut start: u32, mut end: u32) -> Result<StringArray> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end)?;
        let pubkeys = to_value(&pubkeys)?;

        Ok(pubkeys.into())
    }

    #[wasm_bindgen(js_name=toString)]
    pub fn to_string(&self) -> Result<String> {
        Ok(self.hd_wallet.to_string(None).to_string())
    }
}
