use crate::imports::*;

///
/// Extended private key (XPrv).
///
/// This class allows accepts a master seed and provides
/// functions for derivation of dependent child private keys.
///
/// Please note that Kaspa extended private keys use `kprv` prefix.
///
/// @see {@link PrivateKeyGenerator}, {@link PublicKeyGenerator}, {@link XPub}, {@link Mnemonic}
/// @category Wallet SDK
///
#[wasm_bindgen]
pub struct XPrv {
    inner: ExtendedPrivateKey<SecretKey>,
}

#[wasm_bindgen]
impl XPrv {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: String) -> Result<XPrv> {
        let seed_bytes = Vec::<u8>::from_hex(&seed).map_err(|_| Error::custom("Invalid seed"))?;

        let inner = ExtendedPrivateKey::<SecretKey>::new(seed_bytes)?;
        Ok(Self { inner })
    }

    /// Create {@link XPrv} from `xprvxxxx..` string
    #[wasm_bindgen(js_name=fromXPrv)]
    pub fn from_xprv_str(xprv: String) -> Result<XPrv> {
        Ok(Self { inner: ExtendedPrivateKey::<SecretKey>::from_str(&xprv)? })
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

    #[wasm_bindgen(js_name = intoString)]
    pub fn into_string(&self, prefix: &str) -> Result<String> {
        let str = self.inner.to_extended_key(prefix.try_into()?).to_string();
        Ok(str)
    }
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string(&self) -> Result<String> {
        let str = self.inner.to_extended_key("kprv".try_into()?).to_string();
        Ok(str)
    }

    #[wasm_bindgen(js_name = toXPub)]
    pub fn to_xpub(&self) -> Result<XPub> {
        let publick_key = self.inner.public_key();
        Ok(publick_key.into())
    }
}
