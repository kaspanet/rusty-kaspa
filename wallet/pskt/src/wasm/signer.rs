// todo: this is a copy/paste from wallet/core/src/wasm
// i tried to mutualize it in wallet/core/keys, but it conflicted (circular dep with tx_script)
// it also feels overkill to create a package only for that
// need guidance on how to procede with architecturing

use js_sys::Array;
use kaspa_wallet_keys::privatekey::PrivateKey;
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::TryCastFromJs;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Array, is_type_of = Array::is_array, typescript_type = "(PrivateKey | HexString | Uint8Array)[]")]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub type PrivateKeyArrayT;
}

impl TryFrom<PrivateKeyArrayT> for Vec<PrivateKey> {
    type Error = crate::error::Error;
    fn try_from(keys: PrivateKeyArrayT) -> std::result::Result<Self, Self::Error> {
        let mut private_keys: Vec<PrivateKey> = vec![];
        for key in keys.iter() {
            private_keys
                .push(PrivateKey::try_owned_from(key).map_err(|_| Self::Error::Custom("Unable to cast PrivateKey".to_string()))?);
        }

        Ok(private_keys)
    }
}
