use crate::result::Result;
use kaspa_consensus_core::network::{NetworkType, NetworkTypeT};
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "bigint | number | HexString")]
    #[derive(Clone, Debug)]
    pub type ISompiToKaspa;
}

///
/// Convert a Sompi represented by bigint to Kaspa floating point amount.
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = "sompiToKaspa")]
pub fn sompi_to_kaspa(sompi: ISompiToKaspa) -> Result<f64> {
    let sompi = sompi.try_as_u64()?;
    Ok(crate::utils::sompi_to_kaspa(sompi))
}

///
/// Convert a Kaspa floating point amount to Sompi represented by bigint.
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = "kaspaToSompi")]
pub fn kaspa_to_sompi(kaspa: f64) -> u64 {
    crate::utils::kaspa_to_sompi(kaspa)
}

///
/// Convert Sompi to a string representation of the amount in Kaspa.
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = "sompiToKaspaString")]
pub fn sompi_to_kaspa_string(sompi: ISompiToKaspa) -> Result<String> {
    let sompi = sompi.try_as_u64()?;
    Ok(crate::utils::sompi_to_kaspa_string(sompi))
}

///
/// Format a Sompi amount to a string representation of the amount in Kaspa with a suffix
/// based on the network type (e.g. `KAS` for mainnet, `TKAS` for testnet,
/// `SKAS` for simnet, `DKAS` for devnet).
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = "sompiToKaspaStringWithSuffix")]
pub fn sompi_to_kaspa_string_with_suffix(sompi: ISompiToKaspa, network: NetworkTypeT) -> Result<String> {
    let sompi = sompi.try_as_u64()?;
    let network_type = NetworkType::try_from(network)?;
    Ok(crate::utils::sompi_to_kaspa_string_with_suffix(sompi, &network_type))
}
