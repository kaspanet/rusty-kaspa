pub use crate::viewer::ScriptViewerOptions;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmScriptViewerOptions {
    inner: ScriptViewerOptions,
}

#[wasm_bindgen]
impl WasmScriptViewerOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(contains_redeem_script: bool) -> WasmScriptViewerOptions {
        WasmScriptViewerOptions { inner: { ScriptViewerOptions { contains_redeem_script } } }
    }

    #[wasm_bindgen(getter)]
    pub fn contains_redeem_script(&self) -> bool {
        self.inner.contains_redeem_script
    }
}

impl From<&WasmScriptViewerOptions> for ScriptViewerOptions {
    fn from(value: &WasmScriptViewerOptions) -> Self {
        value.inner.clone()
    }
}
