use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::ObjectExtension;

use crate::{error::Error, result::Result, viewer::ScriptViewerOptions as NativeScriptViewerOptions};
use kaspa_addresses::Prefix;

#[wasm_bindgen(typescript_custom_section)]
const TS_SCRIPT_VIEWER_OPTIONS: &'static str = r#"
/**
 * Interface defining the structure of a script viewer option.
 *
 * @category txscript
 */
export interface ScriptViewerOptions {
    /** Wether or not to try disassemble sub-script (redeem script) */
    contains_redeem_script?: boolean;
    /** Address prefix used when formatting signer addresses. Defaults to `kaspa`. */
    address_prefix?: string;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "ScriptViewerOptions")]
    pub type ScriptViewerOptions;
}

impl TryFrom<ScriptViewerOptions> for NativeScriptViewerOptions {
    type Error = Error;

    fn try_from(value: ScriptViewerOptions) -> Result<Self> {
        let object = js_sys::Object::try_from(&value).ok_or_else(|| Error::Custom("options must be an object".into()))?;
        let address_prefix = object
            .get_string("address_prefix")
            .ok()
            .map(|prefix| Prefix::try_from(prefix.as_str()).map_err(|err| Error::convert("address_prefix", err)))
            .transpose()?
            .unwrap_or(Prefix::Mainnet);

        Ok(Self { contains_redeem_script: object.get_bool("contains_redeem_script").ok().unwrap_or(false), address_prefix })
    }
}
