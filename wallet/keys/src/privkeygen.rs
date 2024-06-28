use crate::derivation::gen1::WalletDerivationManager;
use crate::imports::*;

///
/// Helper class to generate private keys from an extended private key (XPrv).
/// This class accepts the master Kaspa XPrv string (e.g. `xprv1...`) and generates
/// private keys for the receive and change paths given the pre-set parameters
/// such as account index, multisig purpose and cosigner index.
///
/// Please note that in Kaspa master private keys use `kprv` prefix.
///
/// @see {@link PublicKeyGenerator}, {@link XPub}, {@link XPrv}, {@link Mnemonic}
/// @category Wallet SDK
#[cfg_attr(feature = "py-sdk", pyclass)]
#[wasm_bindgen]
pub struct PrivateKeyGenerator {
    receive: ExtendedPrivateKey<SecretKey>,
    change: ExtendedPrivateKey<SecretKey>,
}

// PY-NOTE: WASM specific fn implementations
#[wasm_bindgen]
impl PrivateKeyGenerator {
    #[wasm_bindgen(constructor)]
    pub fn new(xprv: &XPrvT, is_multisig: bool, account_index: u64, cosigner_index: Option<u32>) -> Result<PrivateKeyGenerator> {
        let xprv = XPrv::try_cast_from(xprv)?;
        let xprv = xprv.as_ref().inner();
        let receive = xprv.clone().derive_path(&WalletDerivationManager::build_derivate_path(
            is_multisig,
            account_index,
            cosigner_index,
            Some(kaspa_bip32::AddressType::Receive),
        )?)?;
        let change = xprv.clone().derive_path(&WalletDerivationManager::build_derivate_path(
            is_multisig,
            account_index,
            cosigner_index,
            Some(kaspa_bip32::AddressType::Change),
        )?)?;

        Ok(Self { receive, change })
    }
}

// PY-NOTE: fns exposed to both WASM and Python
#[cfg_attr(feature = "py-sdk", pymethods)]
#[wasm_bindgen]
impl PrivateKeyGenerator {
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

// PY-NOTE: Python specific fn implementations
#[cfg(feature = "py-sdk")]
#[pymethods]
impl PrivateKeyGenerator {
    // PY-NOTE: #[new] has to be in block that has #[pymethods] applied directly. applying via #[cfg_attr()] does not work (PyO3 limitation).
    #[new]
    pub fn new_py(xprv: String, is_multisig: bool, account_index: u64, cosigner_index: Option<u32>) -> Result<PrivateKeyGenerator> {
        // PY-NOTE: accepting `xprv: String` instead of `xprv: XPrvT` due to challenges with XPrvT type when building python bindings
        let xprv = XPrv::from_xprv_str(xprv)?;
        let xprv = xprv.inner();
        let receive = xprv.clone().derive_path(&WalletDerivationManager::build_derivate_path(
            is_multisig,
            account_index,
            cosigner_index,
            Some(kaspa_bip32::AddressType::Receive),
        )?)?;
        let change = xprv.clone().derive_path(&WalletDerivationManager::build_derivate_path(
            is_multisig,
            account_index,
            cosigner_index,
            Some(kaspa_bip32::AddressType::Change),
        )?)?;

        Ok(Self { receive, change })
    }
}
