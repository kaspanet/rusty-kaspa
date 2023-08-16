use crate::accounts::gen1::WalletDerivationManager;
use crate::Result;
use kaspa_bip32::{ChildNumber, ExtendedPrivateKey, SecretKey};
use kaspa_consensus_wasm::PrivateKey;
use std::str::FromStr;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct XPrivateKey {
    receive: ExtendedPrivateKey<SecretKey>,
    change: ExtendedPrivateKey<SecretKey>,
}
#[wasm_bindgen]
impl XPrivateKey {
    #[wasm_bindgen(constructor)]
    pub fn new(xprv: &str, is_multisig: bool, account_index: u64, cosigner_index: Option<u32>) -> Result<XPrivateKey> {
        let xkey = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?;
        let receive = xkey.clone().derive_path(WalletDerivationManager::build_derivate_path(
            is_multisig,
            account_index,
            cosigner_index,
            Some(kaspa_bip32::AddressType::Receive),
        )?)?;
        let change = xkey.derive_path(WalletDerivationManager::build_derivate_path(
            is_multisig,
            account_index,
            cosigner_index,
            Some(kaspa_bip32::AddressType::Change),
        )?)?;

        Ok(Self { receive, change })
    }

    #[wasm_bindgen(js_name=receiveKey)]
    pub fn receive_key(&self, index: u32) -> Result<PrivateKey> {
        let xkey = self.receive.derive_child(ChildNumber::new(index, false)?)?;
        Ok(PrivateKey::from(xkey.private_key()))
    }

    #[wasm_bindgen(js_name=changeKey)]
    pub fn change_key(&self, index: u32) -> Result<PrivateKey> {
        let xkey = self.change.derive_child(ChildNumber::new(index, false)?)?;
        Ok(PrivateKey::from(xkey.private_key()))
    }
}
