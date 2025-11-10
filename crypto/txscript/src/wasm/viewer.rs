use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::ObjectExtension;

use crate::{error::Error, result::Result, viewer::ScriptViewerOptions};

#[wasm_bindgen(typescript_custom_section)]
const TS_SCRIPT_VIEWER_OPTIONS: &'static str = r#"
/**
 * Interface defining the structure of a transaction.
 *
 * @category txscript
 */
export interface IScriptViewerOptions {
    /** Wether or not to try disassemble sub-script (redeem script) */
    contains_redeem_script?: bool;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "IScriptViewerOptions")]
    pub type IScriptViewerOptions;
}

impl TryFrom<IScriptViewerOptions> for ScriptViewerOptions {
    type Error = Error;
    fn try_from(js_value: IScriptViewerOptions) -> Result<ScriptViewerOptions> {
        let object = js_sys::Object::try_from(&js_value).ok_or_else(|| Error::Custom("options must be an object".into()))?;

        let contains_redeem_script = object.get_bool("contains_redeem_script").ok().unwrap_or(false);

        Ok(ScriptViewerOptions { contains_redeem_script })
    }
}
