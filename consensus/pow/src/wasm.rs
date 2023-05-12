use kaspa_consensus_core::header::Header;
use kaspa_math::wasm::Uint256;
use wasm_bindgen::prelude::*;
// use workflow_wasm::jsvalue::*;
use workflow_wasm::result::Result;

#[wasm_bindgen]
pub struct State {
    inner: crate::State,
}

#[wasm_bindgen]
impl State {
    #[wasm_bindgen(constructor)]
    pub fn new(header: &Header) -> Self {
        Self { inner: crate::State::new(header) }
    }

    #[wasm_bindgen(js_name=checkPow)]
    pub fn check_pow(&self, nonce_jsv: JsValue) -> Result<js_sys::Array> {
        // let nonce = nonce_jsv.try_as_u64()?;
        let nonce = try_as_u64_local(nonce_jsv)?;

        let (c, v) = self.inner.check_pow(nonce);
        let array = js_sys::Array::new();
        array.push(&JsValue::from(c));
        array.push(&Uint256::from(v).into());

        Ok(array)
    }
}

fn try_as_u64_local(v: JsValue) -> Result<u64> {
    use workflow_wasm::error::Error;
    if v.is_string() {
        let hex_str = v.as_string().unwrap();
        if hex_str.len() > 16 {
            Err(Error::WrongSize("try_as_u64(): supplied string must be < 16 chars".to_string()))
        } else {
            let mut out = [0u8; 8];
            let mut input = [b'0'; 16];
            let start = input.len() - hex_str.len();
            input[start..].copy_from_slice(hex_str.as_bytes());
            kaspa_math::uint::faster_hex::hex_decode(&input, &mut out)?;
            Ok(u64::from_be_bytes(out))
        }
    } else if v.is_bigint() {
        Ok(v.clone()
            .try_into()
            .map_err(|err| Error::Convert(format!("try_as_u64(): unable to convert BigInt value to u64: `{v:?}`: {err:?}")))?)
    } else {
        Ok(v.as_f64().ok_or_else(|| Error::WrongType(format!("value is not a number ({v:?})")))? as u64)
    }
}
