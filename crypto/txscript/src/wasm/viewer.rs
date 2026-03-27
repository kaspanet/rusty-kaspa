use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::ObjectExtension;

use crate::{error::Error, result::Result, viewer::ScriptViewerOptions as NativeScriptViewerOptions};

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

        Ok(Self { contains_redeem_script: object.get_bool("contains_redeem_script").ok().unwrap_or(false) })
    }
}
