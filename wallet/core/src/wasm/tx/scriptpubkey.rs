use crate::error::Error;
use js_sys::Object;
use kaspa_consensus_core::tx::ScriptPublicKey;
use wasm_bindgen::prelude::*;
use workflow_wasm::jsvalue::*;
use workflow_wasm::object::*;

pub trait ScriptPublicKeyTrait {
    fn try_from_jsvalue(js_value: JsValue) -> crate::Result<ScriptPublicKey>;
}

impl ScriptPublicKeyTrait for ScriptPublicKey {
    fn try_from_jsvalue(js_value: JsValue) -> crate::Result<Self> {
        if let Some(object) = Object::try_from(&js_value) {
            let version_value = object.get("version")?;
            let version = if version_value.is_string() {
                let hex_string = version_value.as_string().unwrap();
                if hex_string.len() != 4 {
                    return Err("`ScriptPublicKey::version` must be a string of length 4 (2 byte hex repr)".into());
                }
                u16::from_str_radix(&hex_string, 16).map_err(|_| Error::Custom("error parsing version hex value".into()))?
            } else if let Ok(version) = version_value.try_as_u16() {
                version
            } else {
                return Err(Error::Custom(format!(
                    "`ScriptPublicKey::version` must be a hex string or a 16-bit integer: `{version_value:?}`"
                )));
            };

            let script = object.get_vec_u8("script")?;

            Ok(ScriptPublicKey::new(version, script.into()))
        } else {
            Err("ScriptPublicKey must be an object".into())
        }
    }
}
