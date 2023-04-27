use crate::accounts::account::WalletDerivationManagerTrait;
use crate::accounts::gen1::WalletDerivationManager;
use crate::Result;
use kaspa_bip32::{ExtendedPrivateKey, SecretKey};
//use serde_wasm_bindgen::to_value;
use std::str::FromStr;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct XPublicKey {
    _hd_wallet: WalletDerivationManager,
}
#[wasm_bindgen]
impl XPublicKey {
    // #[wasm_bindgen(constructor)]
    // pub async fn new(kpub: &str, is_multisig: bool, account_index: u64) -> Result<XPublicKey> {
    //     let xpub = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(kpub)?;
    //     Self::from_xpublic_key(xpub, is_multisig, account_index).await
    // }

    #[wasm_bindgen(js_name=fromXPrv)]
    pub async fn from_xprv(xprv: &str, is_multisig: bool, account_index: u64, cosigner_index: Option<u32>) -> Result<XPublicKey> {
        let xprv = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?;
        let path = WalletDerivationManager::build_derivate_path(is_multisig, account_index, None, None)?;
        let xprv = xprv.derive_path(path)?;
        let xpub = xprv.public_key();
        let _hd_wallet = WalletDerivationManager::from_extended_public_key(xpub, cosigner_index).await?;
        Ok(Self { _hd_wallet })
    }

    #[wasm_bindgen(js_name=receiveAddresses)]
    pub async fn receive_addresses(&self, mut _start: u32, mut _end: u32) -> Result<JsValue> {
        // if start > end {
        //     (start, end) = (end, start);
        // }
        // let addresses = self.hd_wallet.receive_address_manager().derive_address_range(start..end).await?;
        // let addresses = to_value(&addresses)?;
        // Ok(addresses)

        Ok("TODO".into())
    }

    #[wasm_bindgen(js_name=changeAddresses)]
    pub async fn change_addresses(&self, mut _start: u32, mut _end: u32) -> Result<JsValue> {
        // if start > end {
        //     (start, end) = (end, start);
        // }
        // let addresses = self.hd_wallet.change_address_manager().derive_address_range(start..end).await?;
        // let addresses = to_value(&addresses)?;
        // Ok(addresses)

        Ok("TODO".into())
    }
}
