use crate::result::Result;
use kaspa_consensus_core::network::NetworkType;
use pyo3::prelude::*;
use std::str::FromStr;

#[pyfunction]
pub fn kaspa_to_sompi(kaspa: f64) -> u64 {
    crate::utils::kaspa_to_sompi(kaspa)
}

#[pyfunction]
pub fn sompi_to_kaspa(sompi: u64) -> f64 {
    crate::utils::sompi_to_kaspa(sompi)
}

#[pyfunction]
pub fn sompi_to_kaspa_string_with_suffix(sompi: u64, network: &str) -> Result<String> {
    let network_type = NetworkType::from_str(network)?;
    Ok(crate::utils::sompi_to_kaspa_string_with_suffix(sompi, &network_type))
}
