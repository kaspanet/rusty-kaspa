//!
//! [`PublicKeyGenerator`] helper for generating public key derivations from an extended public key (XPub).
//!

use crate::derivation::gen1::WalletDerivationManager;
use crate::derivation::traits::WalletDerivationManagerTrait;
use crate::imports::*;
use kaspa_addresses::AddressArrayT;
use kaspa_consensus_core::network::NetworkType;
// use crate::xprv::XPrv;

///
/// Helper class to generate public keys from an extended public key (XPub)
/// that has been derived up to the co-signer index.
///
/// Please note that in Kaspa master public keys use `kpub` prefix.
///
/// @see {@link PrivateKeyGenerator}, {@link XPub}, {@link XPrv}, {@link Mnemonic}
/// @category Wallet SDK
///
#[cfg_attr(feature = "py-sdk", pyclass)]
#[wasm_bindgen]
pub struct PublicKeyGenerator {
    hd_wallet: WalletDerivationManager,
}

#[wasm_bindgen]
impl PublicKeyGenerator {
    #[wasm_bindgen(js_name=fromXPub)]
    pub fn from_xpub(kpub: &XPubT, cosigner_index: Option<u32>) -> Result<PublicKeyGenerator> {
        let kpub = XPub::try_cast_from(kpub)?;
        let xpub = kpub.as_ref().inner();
        let hd_wallet = WalletDerivationManager::from_extended_public_key(xpub.clone(), cosigner_index)?;
        Ok(Self { hd_wallet })
    }

    #[wasm_bindgen(js_name=fromMasterXPrv)]
    pub fn from_master_xprv(
        xprv: &XPrvT,
        is_multisig: bool,
        account_index: u64,
        cosigner_index: Option<u32>,
    ) -> Result<PublicKeyGenerator> {
        let path = WalletDerivationManager::build_derivate_path(is_multisig, account_index, None, None)?;
        let xprv = XPrv::try_owned_from(xprv)?.inner().clone().derive_path(&path)?;
        let xpub = xprv.public_key();
        let hd_wallet = WalletDerivationManager::from_extended_public_key(xpub, cosigner_index)?;
        Ok(Self { hd_wallet })
    }

    // ---

    /// Generate Receive Public Key derivations for a given range.
    #[wasm_bindgen(js_name=receivePubkeys)]
    pub fn receive_pubkeys(&self, mut start: u32, mut end: u32) -> Result<PublicKeyArrayT> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.receive_pubkey_manager().derive_pubkey_range(start..end)?;
        Ok(Array::from_iter(pubkeys.into_iter().map(|pk| JsValue::from(PublicKey::from(pk)))).unchecked_into())
    }

    /// Generate a single Receive Public Key derivation at a given index.
    #[wasm_bindgen(js_name=receivePubkey)]
    pub fn receive_pubkey(&self, index: u32) -> Result<PublicKey> {
        Ok(self.hd_wallet.receive_pubkey_manager().derive_pubkey(index)?.into())
    }

    /// Generate a range of Receive Public Key derivations and return them as strings.
    #[wasm_bindgen(js_name=receivePubkeysAsStrings)]
    pub fn receive_pubkeys_as_strings(&self, mut start: u32, mut end: u32) -> Result<StringArray> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.receive_pubkey_manager().derive_pubkey_range(start..end)?;
        Ok(Array::from_iter(pubkeys.into_iter().map(|pk| JsValue::from(PublicKey::from(pk).to_string()))).unchecked_into())
    }

    /// Generate a single Receive Public Key derivation at a given index and return it as a string.
    #[wasm_bindgen(js_name=receivePubkeyAsString)]
    pub fn receive_pubkey_as_string(&self, index: u32) -> Result<String> {
        Ok(self.hd_wallet.receive_pubkey_manager().derive_pubkey(index)?.to_string())
    }

    /// Generate Receive Address derivations for a given range.
    #[wasm_bindgen(js_name=receiveAddresses)]
    #[allow(non_snake_case)]
    pub fn receive_addresses(&self, networkType: &NetworkTypeT, mut start: u32, mut end: u32) -> Result<AddressArrayT> {
        if start > end {
            (start, end) = (end, start);
        }
        let network_type = NetworkType::try_from(networkType)?;
        let pubkeys = self.hd_wallet.receive_pubkey_manager().derive_pubkey_range(start..end)?;
        let addresses =
            pubkeys.into_iter().map(|pk| PublicKey::from(pk).to_address(network_type)).collect::<Result<Vec<Address>>>()?;
        Ok(Array::from_iter(addresses.into_iter().map(JsValue::from)).unchecked_into())
    }

    /// Generate a single Receive Address derivation at a given index.
    #[wasm_bindgen(js_name=receiveAddress)]
    #[allow(non_snake_case)]
    pub fn receive_address(&self, networkType: &NetworkTypeT, index: u32) -> Result<Address> {
        PublicKey::from(self.hd_wallet.receive_pubkey_manager().derive_pubkey(index)?).to_address(networkType.try_into()?)
    }

    /// Generate a range of Receive Address derivations and return them as strings.
    #[wasm_bindgen(js_name=receiveAddressAsStrings)]
    #[allow(non_snake_case)]
    pub fn receive_addresses_as_strings(&self, networkType: &NetworkTypeT, mut start: u32, mut end: u32) -> Result<StringArray> {
        if start > end {
            (start, end) = (end, start);
        }
        let network_type = NetworkType::try_from(networkType)?;
        let pubkeys = self.hd_wallet.receive_pubkey_manager().derive_pubkey_range(start..end)?;
        let addresses =
            pubkeys.into_iter().map(|pk| PublicKey::from(pk).to_address(network_type)).collect::<Result<Vec<Address>>>()?;
        Ok(Array::from_iter(addresses.into_iter().map(String::from).map(JsValue::from)).unchecked_into())
    }

    /// Generate a single Receive Address derivation at a given index and return it as a string.
    #[wasm_bindgen(js_name=receiveAddressAsString)]
    #[allow(non_snake_case)]
    pub fn receive_address_as_string(&self, networkType: &NetworkTypeT, index: u32) -> Result<String> {
        Ok(PublicKey::from(self.hd_wallet.receive_pubkey_manager().derive_pubkey(index)?)
            .to_address(networkType.try_into()?)?
            .to_string())
    }

    // ---

    /// Generate Change Public Key derivations for a given range.
    #[wasm_bindgen(js_name=changePubkeys)]
    pub fn change_pubkeys(&self, mut start: u32, mut end: u32) -> Result<PublicKeyArrayT> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end)?;
        Ok(Array::from_iter(pubkeys.into_iter().map(|pk| JsValue::from(PublicKey::from(pk)))).unchecked_into())
    }

    /// Generate a single Change Public Key derivation at a given index.
    #[wasm_bindgen(js_name=changePubkey)]
    pub fn change_pubkey(&self, index: u32) -> Result<PublicKey> {
        Ok(self.hd_wallet.change_pubkey_manager().derive_pubkey(index)?.into())
    }

    /// Generate a range of Change Public Key derivations and return them as strings.
    #[wasm_bindgen(js_name=changePubkeysAsStrings)]
    pub fn change_pubkeys_as_strings(&self, mut start: u32, mut end: u32) -> Result<StringArray> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end)?;
        Ok(Array::from_iter(pubkeys.into_iter().map(|pk| JsValue::from(PublicKey::from(pk).to_string()))).unchecked_into())
    }

    /// Generate a single Change Public Key derivation at a given index and return it as a string.
    #[wasm_bindgen(js_name=changePubkeyAsString)]
    pub fn change_pubkey_as_string(&self, index: u32) -> Result<String> {
        Ok(self.hd_wallet.change_pubkey_manager().derive_pubkey(index)?.to_string())
    }

    /// Generate Change Address derivations for a given range.
    #[wasm_bindgen(js_name=changeAddresses)]
    #[allow(non_snake_case)]
    pub fn change_addresses(&self, networkType: &NetworkTypeT, mut start: u32, mut end: u32) -> Result<AddressArrayT> {
        if start > end {
            (start, end) = (end, start);
        }
        let network_type = NetworkType::try_from(networkType)?;
        let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end)?;
        let addresses =
            pubkeys.into_iter().map(|pk| PublicKey::from(pk).to_address(network_type)).collect::<Result<Vec<Address>>>()?;
        Ok(Array::from_iter(addresses.into_iter().map(JsValue::from)).unchecked_into())
    }

    /// Generate a single Change Address derivation at a given index.
    #[wasm_bindgen(js_name=changeAddress)]
    #[allow(non_snake_case)]
    pub fn change_address(&self, networkType: &NetworkTypeT, index: u32) -> Result<Address> {
        PublicKey::from(self.hd_wallet.change_pubkey_manager().derive_pubkey(index)?).to_address(networkType.try_into()?)
    }

    /// Generate a range of Change Address derivations and return them as strings.
    #[wasm_bindgen(js_name=changeAddressAsStrings)]
    #[allow(non_snake_case)]
    pub fn change_addresses_as_strings(&self, networkType: &NetworkTypeT, mut start: u32, mut end: u32) -> Result<StringArray> {
        if start > end {
            (start, end) = (end, start);
        }
        let network_type = NetworkType::try_from(networkType)?;
        let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end)?;
        let addresses =
            pubkeys.into_iter().map(|pk| PublicKey::from(pk).to_address(network_type)).collect::<Result<Vec<Address>>>()?;
        Ok(Array::from_iter(addresses.into_iter().map(String::from).map(JsValue::from)).unchecked_into())
    }

    /// Generate a single Change Address derivation at a given index and return it as a string.
    #[wasm_bindgen(js_name=changeAddressAsString)]
    #[allow(non_snake_case)]
    pub fn change_address_as_string(&self, networkType: &NetworkTypeT, index: u32) -> Result<String> {
        Ok(PublicKey::from(self.hd_wallet.change_pubkey_manager().derive_pubkey(index)?)
            .to_address(networkType.try_into()?)?
            .to_string())
    }

    // #[wasm_bindgen(js_name=changePubkeys)]
    // pub fn change_pubkeys(&self, mut start: u32, mut end: u32) -> Result<PublicKeyArrayT> {
    //     if start > end {
    //         (start, end) = (end, start);
    //     }
    //     let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end)?;
    //     Ok(Array::from_iter(pubkeys.into_iter().map(|pk| JsValue::from(PublicKey::from(pk)))).unchecked_into())
    // }

    // #[wasm_bindgen(js_name=changePubkey)]
    // pub fn change_pubkey(&self, index: u32) -> Result<PublicKey> {
    //     Ok(self.hd_wallet.change_pubkey_manager().derive_pubkey(index)?.into())
    // }

    #[wasm_bindgen(js_name=toString)]
    pub fn to_string(&self) -> Result<String> {
        Ok(self.hd_wallet.to_string(None).to_string())
    }
}

#[cfg(feature = "py-sdk")]
#[pymethods]
impl PublicKeyGenerator {
    #[staticmethod]
    #[pyo3(name = "from_xpub")]
    #[pyo3(signature = (kpub, cosigner_index=None))]
    fn from_xpub_py(kpub: &str, cosigner_index: Option<u32>) -> PyResult<PublicKeyGenerator> {
        let kpub = XPub::try_new(kpub)?;
        let xpub = kpub.inner();
        let hd_wallet = WalletDerivationManager::from_extended_public_key(xpub.clone(), cosigner_index)?;
        Ok(Self { hd_wallet })
    }

    #[staticmethod]
    #[pyo3(name = "from_master_xprv")]
    #[pyo3(signature = (xprv, is_multisig, account_index, cosigner_index=None))]
    fn from_master_xprv_py(
        xprv: String,
        is_multisig: bool,
        account_index: u64,
        cosigner_index: Option<u32>,
    ) -> PyResult<PublicKeyGenerator> {
        let path = WalletDerivationManager::build_derivate_path(is_multisig, account_index, None, None)?;
        let xprv = XPrv::from_xprv_str(xprv)?.inner().clone().derive_path(&path)?;
        let xpub = xprv.public_key();
        let hd_wallet = WalletDerivationManager::from_extended_public_key(xpub, cosigner_index)?;
        Ok(Self { hd_wallet })
    }

    #[pyo3(name = "receive_pubkeys")]
    fn receive_pubkeys_py(&self, mut start: u32, mut end: u32) -> PyResult<Vec<PublicKey>> {
        if start > end {
            (start, end) = (end, start)
        }
        let pubkeys = self.hd_wallet.receive_pubkey_manager().derive_pubkey_range(start..end)?;
        Ok(pubkeys.into_iter().map(|pk| PublicKey::from(pk)).collect())
    }

    #[pyo3(name = "receive_pubkey")]
    pub fn receive_pubkey_py(&self, index: u32) -> PyResult<PublicKey> {
        Ok(self.hd_wallet.receive_pubkey_manager().derive_pubkey(index)?.into())
    }

    #[pyo3(name = "receive_pubkeys_as_strings")]
    fn receive_pubkeys_as_strings_py(&self, mut start: u32, mut end: u32) -> PyResult<Vec<String>> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.receive_pubkey_manager().derive_pubkey_range(start..end)?;
        Ok(pubkeys.into_iter().map(|pk| PublicKey::from(pk).to_string()).collect())
    }

    #[pyo3(name = "receive_pubkey_as_string")]
    pub fn receive_pubkey_as_string_py(&self, index: u32) -> PyResult<String> {
        Ok(self.hd_wallet.receive_pubkey_manager().derive_pubkey(index)?.to_string())
    }

    #[pyo3(name = "receive_addresses")]
    fn receive_addresses_py(&self, network_type: &str, mut start: u32, mut end: u32) -> PyResult<Vec<Address>> {
        if start > end {
            (start, end) = (end, start);
        }
        let network_type = NetworkType::from_str(network_type)?;
        let pubkeys = self.hd_wallet.receive_pubkey_manager().derive_pubkey_range(start..end)?;
        let addresses =
            pubkeys.into_iter().map(|pk| PublicKey::from(pk).to_address(network_type)).collect::<Result<Vec<Address>>>()?;
        Ok(addresses)
    }

    #[pyo3(name = "receive_address")]
    fn receive_address_py(&self, network_type: &str, index: u32) -> PyResult<Address> {
        let network_type = NetworkType::from_str(network_type)?;
        Ok(PublicKey::from(self.hd_wallet.receive_pubkey_manager().derive_pubkey(index)?).to_address(network_type)?)
    }

    #[pyo3(name = "receive_addresses_as_strings")]
    fn receive_addresses_as_strings_py(&self, network_type: &str, mut start: u32, mut end: u32) -> PyResult<Vec<String>> {
        if start > end {
            (start, end) = (end, start);
        }
        let network_type = NetworkType::from_str(network_type)?;
        let pubkeys = self.hd_wallet.receive_pubkey_manager().derive_pubkey_range(start..end)?;
        let addresses =
            pubkeys.into_iter().map(|pk| PublicKey::from(pk).to_address(network_type)).collect::<Result<Vec<Address>>>()?;
        Ok(addresses.into_iter().map(|a| a.address_to_string()).collect())
    }

    #[pyo3(name = "receive_address_as_string")]
    fn receive_address_as_string_py(&self, network_type: &str, index: u32) -> PyResult<String> {
        Ok(PublicKey::from(self.hd_wallet.receive_pubkey_manager().derive_pubkey(index)?)
            .to_address(NetworkType::from_str(network_type)?)?
            .to_string())
    }

    #[pyo3(name = "change_pubkeys")]
    pub fn change_pubkeys_py(&self, mut start: u32, mut end: u32) -> PyResult<Vec<PublicKey>> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end)?;
        Ok(pubkeys.into_iter().map(|pk| PublicKey::from(pk)).collect())
    }

    #[pyo3(name = "change_pubkey")]
    pub fn change_pubkey_py(&self, index: u32) -> PyResult<PublicKey> {
        Ok(self.hd_wallet.change_pubkey_manager().derive_pubkey(index)?.into())
    }

    #[pyo3(name = "change_pubkeys_as_strings")]
    pub fn change_pubkeys_as_strings_py(&self, mut start: u32, mut end: u32) -> PyResult<Vec<String>> {
        if start > end {
            (start, end) = (end, start);
        }
        let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end)?;
        Ok(pubkeys.into_iter().map(|pk| PublicKey::from(pk).to_string()).collect())
    }

    #[pyo3(name = "change_pubkey_as_string")]
    pub fn change_pubkey_as_string_py(&self, index: u32) -> PyResult<String> {
        Ok(self.hd_wallet.change_pubkey_manager().derive_pubkey(index)?.to_string())
    }

    #[pyo3(name = "change_addresses")]
    pub fn change_addresses_py(&self, network_type: &str, mut start: u32, mut end: u32) -> PyResult<Vec<Address>> {
        if start > end {
            (start, end) = (end, start);
        }
        let network_type = NetworkType::from_str(network_type)?;
        let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end)?;
        let addresses =
            pubkeys.into_iter().map(|pk| PublicKey::from(pk).to_address(network_type)).collect::<Result<Vec<Address>>>()?;
        Ok(addresses)
    }

    #[pyo3(name = "change_address")]
    pub fn change_address_py(&self, network_type: &str, index: u32) -> PyResult<Address> {
        let network_type = NetworkType::from_str(network_type)?;
        Ok(PublicKey::from(self.hd_wallet.change_pubkey_manager().derive_pubkey(index)?).to_address(network_type)?)
    }

    #[pyo3(name = "change_addresses_as_strings")]
    pub fn change_addresses_as_strings_py(&self, network_type: &str, mut start: u32, mut end: u32) -> PyResult<Vec<String>> {
        if start > end {
            (start, end) = (end, start);
        }
        let network_type = NetworkType::from_str(network_type)?;
        let pubkeys = self.hd_wallet.change_pubkey_manager().derive_pubkey_range(start..end)?;
        let addresses =
            pubkeys.into_iter().map(|pk| PublicKey::from(pk).to_address(network_type)).collect::<Result<Vec<Address>>>()?;
        Ok(addresses.into_iter().map(|a| a.address_to_string()).collect())
    }

    #[pyo3(name = "change_address_as_string")]
    pub fn change_address_as_string_py(&self, network_type: &str, index: u32) -> PyResult<String> {
        Ok(PublicKey::from(self.hd_wallet.receive_pubkey_manager().derive_pubkey(index)?)
            .to_address(NetworkType::from_str(network_type)?)?
            .to_string())
    }

    #[pyo3(name = "to_string")]
    pub fn to_string_py(&self) -> PyResult<String> {
        Ok(self.hd_wallet.to_string(None).to_string())
    }
}
