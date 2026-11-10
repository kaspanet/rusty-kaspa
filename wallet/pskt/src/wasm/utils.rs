use aero_consensus_core::constants::*;
use aero_consensus_core::network::NetworkType;
use separator::{Separatable, separated_float, separated_int, separated_uint_with_output};

#[inline]
pub fn sompi_to_aero(sompi: u64) -> f64 {
    sompi as f64 / SOMPI_PER_AERO as f64
}

#[inline]
pub fn aero_to_sompi(aero: f64) -> u64 {
    (aero * SOMPI_PER_AERO as f64) as u64
}

#[inline]
pub fn sompi_to_aero_string(sompi: u64) -> String {
    sompi_to_aero(sompi).separated_string()
}

#[inline]
pub fn sompi_to_aero_string_with_trailing_zeroes(sompi: u64) -> String {
    separated_float!(format!("{:.8}", sompi_to_aero(sompi)))
}

pub fn aero_suffix(network_type: &NetworkType) -> &'static str {
    match network_type {
        NetworkType::Mainnet => "ARO",
        NetworkType::Testnet => "TARO",
        NetworkType::Simnet => "SARO",
        NetworkType::Devnet => "DARO",
    }
}

#[inline]
pub fn sompi_to_aero_string_with_suffix(sompi: u64, network_type: &NetworkType) -> String {
    let kas = sompi_to_aero_string(sompi);
    let suffix = aero_suffix(network_type);
    format!("{kas} {suffix}")
}
