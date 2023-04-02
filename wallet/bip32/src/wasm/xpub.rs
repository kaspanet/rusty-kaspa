use crate::{wasm::DerivationPath, ChildNumber, ExtendedPublicKey, Result};
use secp256k1::PublicKey;
use std::str::FromStr;
use wasm_bindgen::prelude::*;
use js_sys::Array;

#[wasm_bindgen]
pub struct XPub {
    inner: ExtendedPublicKey<PublicKey>,
}

#[wasm_bindgen]
impl XPub {
    #[wasm_bindgen(constructor)]
    pub fn new(xpub: &str) -> Result<XPub> {
        let inner = ExtendedPublicKey::<PublicKey>::from_str(xpub)?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(js_name=deriveChild)]
    pub fn derive_child(&self, chile_number: u32, hardened: Option<bool>) -> Result<XPub> {
        let chile_number = ChildNumber::new(chile_number, hardened.unwrap_or(false))?;
        let inner = self.inner.derive_child(chile_number)?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(js_name=derivePath)]
    pub fn derive_path(&self, path: JsValue) -> Result<XPub> {
        let path = DerivationPath::try_from(path)?;
        let inner = self.inner.clone().derive_path(path.into())?;
        Ok(Self { inner })
    }

    //#[wasm_bindgen(js_name = toString)]
    #[wasm_bindgen(js_name = intoString)]
    pub fn to_str(&self, prefix: &str) -> Result<String> {
        Ok(self.inner.to_string(Some(prefix.try_into()?)))
    }

    #[wasm_bindgen(js_name = toBytes)]
    pub fn to_bytes(&self)->Array{
        let array = js_sys::Uint8Array::from(&self.inner.to_bytes());
        Array::from_iter(self.inner.to_bytes().iter().map(JsValue::from))
    }
}

impl From<ExtendedPublicKey<PublicKey>> for XPub {
    fn from(inner: ExtendedPublicKey<PublicKey>) -> Self {
        Self { inner }
    }
}
