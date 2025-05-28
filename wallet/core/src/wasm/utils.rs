use crate::imports::NetworkParams;
use crate::result::Result;
use crate::wasm::api::message::INetworkParams;
use js_sys::BigInt;
use kaspa_consensus_core::network::{NetworkIdT, NetworkType, NetworkTypeT};
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "bigint | number | HexString")]
    #[derive(Clone, Debug)]
    pub type ISompiToKaspa;
}

/// Convert a Kaspa string to Sompi represented by bigint.
/// This function provides correct precision handling and
/// can be used to parse user input.
/// @category Wallet SDK
#[wasm_bindgen(js_name = "kaspaToSompi")]
pub fn kaspa_to_sompi(kaspa: String) -> Option<BigInt> {
    crate::utils::try_kaspa_str_to_sompi(kaspa).ok().flatten().map(Into::into)
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
pub fn sompi_to_kaspa_string_with_suffix(sompi: ISompiToKaspa, network: &NetworkTypeT) -> Result<String> {
    let sompi = sompi.try_as_u64()?;
    let network_type = NetworkType::try_from(network)?;
    Ok(crate::utils::sompi_to_kaspa_string_with_suffix(sompi, &network_type))
}

#[wasm_bindgen(js_name = "getNetworkParams")]
#[allow(non_snake_case)]
pub fn get_network_params(networkId: NetworkIdT) -> Result<INetworkParams> {
    let params = NetworkParams::from(*networkId.try_into_cast()?);
    params.try_into()
}

#[wasm_bindgen(js_name = "getTransactionMaturityProgress")]
#[allow(non_snake_case)]
pub fn get_transaction_maturity_progress(
    blockDaaScore: BigInt,
    currentDaaScore: BigInt,
    networkId: NetworkIdT,
    isCoinbase: bool,
) -> Result<String> {
    let network_id = *networkId.try_into_cast()?;
    let params = NetworkParams::from(network_id);
    let block_daa_score = blockDaaScore.try_as_u64()?;
    let current_daa_score = currentDaaScore.try_as_u64()?;
    let maturity =
        if isCoinbase { params.coinbase_transaction_maturity_period_daa() } else { params.user_transaction_maturity_period_daa() };

    if current_daa_score < block_daa_score + maturity {
        let progress = (current_daa_score - block_daa_score) as f64 / maturity as f64;
        Ok(format!("{}", (progress * 100.) as usize))
    } else {
        Ok("".to_string())
    }
}
