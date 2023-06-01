use crate::accounts::account::WalletDerivationManagerTrait;
use crate::accounts::gen1::WalletDerivationManager;
use crate::Result;
use kaspa_bip32::{ExtendedPrivateKey, SecretKey};
//use serde_wasm_bindgen::to_value;
use kaspa_bip32::ExtendedPublicKey;
use std::str::FromStr;
use wasm_bindgen::prelude::*;
use workflow_wasm::tovalue::to_value;

#[wasm_bindgen]
pub struct XPublicKey {
    hd_wallet: WalletDerivationManager,
}
#[wasm_bindgen]
impl XPublicKey {
    #[wasm_bindgen(js_name=fromXPub)]
    pub async fn from_xpub(kpub: &str, cosigner_index: Option<u32>) -> Result<XPublicKey> {
        let xpub = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(kpub)?;
        let hd_wallet = WalletDerivationManager::from_extended_public_key(xpub, cosigner_index).await?;
        Ok(Self { hd_wallet })
    }

    #[wasm_bindgen(js_name=fromMasterXPrv)]
    pub async fn from_master_xprv(
        xprv: &str,
        is_multisig: bool,
        account_index: u64,
        cosigner_index: Option<u32>,
    ) -> Result<XPublicKey> {
        let xprv = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?;
        let path = WalletDerivationManager::build_derivate_path(is_multisig, account_index, None, None)?;
        let xprv = xprv.derive_path(path)?;
        let xpub = xprv.public_key();
        let hd_wallet = WalletDerivationManager::from_extended_public_key(xpub, cosigner_index).await?;
        Ok(Self { hd_wallet })
    }

    #[wasm_bindgen(js_name=receivePubkeys)]
    pub async fn receive_pubkeys(&self, mut start: u32, mut end: u32) -> Result<JsValue> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.receive_pubkey_manager().derive_pubkey_range(start..end).await?;
        let pubkeys = to_value(&pubkeys)?;
        Ok(pubkeys)
    }

    #[wasm_bindgen(js_name=changePubkeys)]
    pub async fn change_pubkeys(&self, mut start: u32, mut end: u32) -> Result<JsValue> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end).await?;
        let pubkeys = to_value(&pubkeys)?;

        Ok(pubkeys)
    }
}
