// use crate::tx::{Transaction, TransactionOutput};
use kaspa_addresses::Address;
use kaspa_consensus_core::networktype::NetworkType;
// use kaspa_consensus_core::{
//     config::params::{Params, DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS},
//     constants::*,

//     // mass::{self, MassCalculator},
// };
use kaspa_consensus_core::constants::*;

use separator::Separatable;
use wasm_bindgen::prelude::*;
use workflow_log::style;
// use crate::mass::{self,MassCalculator};

#[inline]
pub fn sompi_to_kaspa(sompi: u64) -> f64 {
    sompi as f64 / SOMPI_PER_KASPA as f64
}

#[inline]
pub fn sompi_to_kaspa_string(sompi: u64) -> String {
    sompi_to_kaspa(sompi).separated_string()
}

pub fn kaspa_suffix(network_type: &NetworkType) -> &'static str {
    match network_type {
        NetworkType::Mainnet => "KAS",
        NetworkType::Testnet => "TKAS",
        NetworkType::Simnet => "SKAS",
        NetworkType::Devnet => "DKAS",
    }
}

#[inline]
pub fn sompi_to_kaspa_string_with_suffix(sompi: u64, network_type: &NetworkType) -> String {
    let kas = sompi_to_kaspa(sompi).separated_string();
    let suffix = kaspa_suffix(network_type);
    format!("{kas} {suffix}")
}

pub fn format_address_colors(address: &Address, range: Option<usize>) -> String {
    let address = address.to_string();

    let parts = address.split(':').collect::<Vec<&str>>();
    let prefix = style(parts[0]).dim();
    let payload = parts[1];
    let range = range.unwrap_or(6);
    let start = range;
    let finish = payload.len() - range;

    let left = &payload[0..start];
    let center = style(&payload[start..finish]).dim();
    let right = &payload[finish..];

    format!("{prefix}:{left}:{center}:{right}")
}

mod wasm {
    use super::*;
    use crate::result::Result;
    // use wasm_bindgen::prelude::*;
    use workflow_wasm::jsvalue::*;
    // use js_sys::BigInt;

    #[wasm_bindgen(js_name = "sompiToKaspa")]
    pub fn sompi_to_kaspa(sompi: JsValue) -> Result<f64> {
        let sompi = sompi.try_as_u64()?;
        Ok(super::sompi_to_kaspa(sompi))
    }

    #[wasm_bindgen(js_name = "sompiToKaspaString")]
    pub fn sompi_to_kaspa_string(sompi: JsValue) -> Result<String> {
        let sompi = sompi.try_as_u64()?;
        Ok(super::sompi_to_kaspa_string(sompi))
    }

    #[wasm_bindgen(js_name = "sompiToKaspaStringWithSuffix")]
    pub fn sompi_to_kaspa_string_with_suffix(sompi: JsValue, wallet: &crate::wasm::Wallet) -> Result<String> {
        let sompi = sompi.try_as_u64()?;
        let network_type = wallet.wallet.network_id()?.network_type;
        Ok(super::sompi_to_kaspa_string_with_suffix(sompi, &network_type))
    }
}
