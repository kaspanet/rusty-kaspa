use crate::{
    wasm::{DerivationPath, XPub},
    ChildNumber, Error, ExtendedPrivateKey, Result, SecretKey,
};
use kaspa_utils::hex::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct XPrv {
    inner: ExtendedPrivateKey<SecretKey>,
}

#[wasm_bindgen]
impl XPrv {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: String) -> Result<XPrv> {
        let seed_bytes = Vec::<u8>::from_hex(&seed).map_err(|_| Error::String("Invalid seed".to_string()))?;

        let inner = ExtendedPrivateKey::<SecretKey>::new(seed_bytes)?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(js_name=deriveChild)]
    pub fn derive_child(&self, chile_number: u32, hardened: Option<bool>) -> Result<XPrv> {
        let chile_number = ChildNumber::new(chile_number, hardened.unwrap_or(false))?;
        let inner = self.inner.derive_child(chile_number)?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(js_name=derivePath)]
    pub fn derive_path(&self, path: JsValue) -> Result<XPrv> {
        let path = DerivationPath::try_from(path)?;
        let inner = self.inner.clone().derive_path(path.into())?;
        Ok(Self { inner })
    }

    //#[wasm_bindgen(js_name = toString)]
    #[wasm_bindgen(js_name = intoString)]
    pub fn to_str(&self, prefix: &str) -> Result<String> {
        let str = self.inner.to_extended_key(prefix.try_into()?).to_string();
        Ok(str)
    }

    #[wasm_bindgen(js_name = publicKey)]
    pub fn public_key(&self) -> Result<XPub> {
        let publick_key = self.inner.public_key();
        Ok(publick_key.into())
    }
}
