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

#[derive(Clone, CastFromJs)]
#[wasm_bindgen]
pub struct XPrv {
    inner: ExtendedPrivateKey<SecretKey>,
}

impl XPrv {
    pub fn inner(&self) -> &ExtendedPrivateKey<SecretKey> {
        &self.inner
    }
}

#[wasm_bindgen]
impl XPrv {
    #[wasm_bindgen(constructor)]
    pub fn try_new(seed: HexString) -> Result<XPrv> {
        let seed_bytes = Vec::<u8>::from_hex(String::try_from(seed)?.as_str()).map_err(|_| Error::custom("Invalid seed"))?;

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
    pub fn derive_path(&self, path: &JsValue) -> Result<XPrv> {
        let path = DerivationPath::try_cast_from(path)?;
        let inner = self.inner.clone().derive_path(path.as_ref().into())?;
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
        let public_key = self.inner.public_key();
        Ok(public_key.into())
    }
}

impl<'a> From<&'a XPrv> for &'a ExtendedPrivateKey<SecretKey> {
    fn from(xprv: &'a XPrv) -> Self {
        &xprv.inner
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "XPrv | string")]
    pub type XPrvT;
}

impl TryCastFromJs for XPrv {
    type Error = Error;
    fn try_cast_from(value: impl AsRef<JsValue>) -> Result<Cast<Self>, Self::Error> {
        Self::resolve(&value, || {
            if let Some(xprv) = value.as_ref().as_string() {
                Ok(XPrv::from_xprv_str(xprv)?)
            } else {
                Err(Error::InvalidXPrv)
            }
        })
    }
}
