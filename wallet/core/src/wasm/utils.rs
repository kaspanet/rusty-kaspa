use crate::result::Result;
use wasm_bindgen::prelude::*;
use workflow_wasm::jsvalue::*;

#[wasm_bindgen(js_name = "sompiToKaspa")]
pub fn sompi_to_kaspa(sompi: JsValue) -> Result<f64> {
    let sompi = sompi.try_as_u64()?;
    Ok(crate::utils::sompi_to_kaspa(sompi))
}

#[wasm_bindgen(js_name = "kaspaToSompi")]
pub fn kaspa_to_sompi(kaspa: f64) -> u64 {
    crate::utils::kaspa_to_sompi(kaspa)
}

#[wasm_bindgen(js_name = "sompiToKaspaString")]
pub fn sompi_to_kaspa_string(sompi: JsValue) -> Result<String> {
    let sompi = sompi.try_as_u64()?;
    Ok(crate::utils::sompi_to_kaspa_string(sompi))
}

#[wasm_bindgen(js_name = "sompiToKaspaStringWithSuffix")]
pub fn sompi_to_kaspa_string_with_suffix(sompi: JsValue, wallet: &crate::wasm::wallet::Wallet) -> Result<String> {
    let sompi = sompi.try_as_u64()?;
    let network_type = wallet.wallet.network_id()?.network_type;
    Ok(crate::utils::sompi_to_kaspa_string_with_suffix(sompi, &network_type))
}
