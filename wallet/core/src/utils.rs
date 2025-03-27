//!
//! Kaspa value formatting and parsing utilities.
//!

use crate::result::Result;
use kaspa_addresses::Address;
use kaspa_consensus_core::constants::*;
use kaspa_consensus_core::network::NetworkType;
use separator::{separated_float, separated_int, separated_uint_with_output, Separatable};
use workflow_log::style;

pub fn try_kaspa_str_to_dwork<S: Into<String>>(s: S) -> Result<Option<u64>> {
    let s: String = s.into();
    let amount = s.trim();
    if amount.is_empty() {
        return Ok(None);
    }

    Ok(Some(str_to_dwork(amount)?))
}

pub fn try_kaspa_str_to_dwork_i64<S: Into<String>>(s: S) -> Result<Option<i64>> {
    let s: String = s.into();
    let amount = s.trim();
    if amount.is_empty() {
        return Ok(None);
    }

    let amount = amount.parse::<f64>()? * DWORK_PER_KASPA as f64;
    Ok(Some(amount as i64))
}

#[inline]
pub fn dwork_to_kaspa(dwork: u64) -> f64 {
    dwork as f64 / DWORK_PER_KASPA as f64
}

#[inline]
pub fn kaspa_to_dwork(kaspa: f64) -> u64 {
    (kaspa * DWORK_PER_KASPA as f64) as u64
}

#[inline]
pub fn dwork_to_kaspa_string(dwork: u64) -> String {
    dwork_to_kaspa(dwork).separated_string()
}

#[inline]
pub fn dwork_to_kaspa_string_with_trailing_zeroes(dwork: u64) -> String {
    separated_float!(format!("{:.8}", dwork_to_kaspa(dwork)))
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
pub fn dwork_to_kaspa_string_with_suffix(dwork: u64, network_type: &NetworkType) -> String {
    let kas = dwork_to_kaspa_string(dwork);
    let suffix = kaspa_suffix(network_type);
    format!("{kas} {suffix}")
}

#[inline]
pub fn dwork_to_kaspa_string_with_trailing_zeroes_and_suffix(dwork: u64, network_type: &NetworkType) -> String {
    let kas = dwork_to_kaspa_string_with_trailing_zeroes(dwork);
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

fn str_to_dwork(amount: &str) -> Result<u64> {
    let Some(dot_idx) = amount.find('.') else {
        return Ok(amount.parse::<u64>()? * DWORK_PER_KASPA);
    };
    let integer = amount[..dot_idx].parse::<u64>()? * DWORK_PER_KASPA;
    let decimal = &amount[dot_idx + 1..];
    let decimal_len = decimal.len();
    let decimal = if decimal_len == 0 {
        0
    } else if decimal_len <= 8 {
        decimal.parse::<u64>()? * 10u64.pow(8 - decimal_len as u32)
    } else {
        // TODO - discuss how to handle values longer than 8 decimal places
        // (reject, truncate, ceil(), etc.)
        decimal[..8].parse::<u64>()?
    };
    Ok(integer + decimal)
}
