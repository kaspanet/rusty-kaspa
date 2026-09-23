//! Shared argument-parsing helpers used by command modules.

use crate::error::{CliError, Result};
use kaspa_rpc_core::RpcAddress;
use serde::de::DeserializeOwned;

/// Resolve a JSON command argument into a typed value.
///
/// The argument is either a literal JSON document or, when prefixed with `@`,
/// a path to a file containing the JSON (`@-` reads stdin). This keeps large
/// inputs such as blocks and transactions out of the shell history.
pub fn json_arg<T: DeserializeOwned>(arg: &str) -> Result<T> {
    let text = read_json_text(arg)?;
    serde_json::from_str(&text).map_err(|e| CliError::Usage(format!("invalid JSON argument: {e}")))
}

/// Read the raw text behind a JSON argument: a literal document, or `@file` /
/// `@-` (stdin) when prefixed with `@`.
fn read_json_text(arg: &str) -> Result<String> {
    if let Some(path) = arg.strip_prefix('@') {
        if path == "-" {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        } else {
            Ok(std::fs::read_to_string(path)?)
        }
    } else {
        Ok(arg.to_string())
    }
}

/// Parse a single bech32 address. Usable as a clap `value_parser`; the error is
/// a `String` so clap can surface it directly.
pub fn parse_address(s: &str) -> std::result::Result<RpcAddress, String> {
    RpcAddress::try_from(s).map_err(|e| format!("invalid address {s:?}: {e}"))
}

/// Parse a list of bech32 addresses, mapping a parse failure to a usage error.
pub fn parse_addresses(addresses: &[String]) -> Result<Vec<RpcAddress>> {
    addresses.iter().map(|a| parse_address(a).map_err(CliError::Usage)).collect()
}

/// Deserialize a JSON command argument (`@file` / `@-` / literal) into a typed
/// value. Usable as a clap `value_parser`; the error is a `String`.
pub fn json_value<T: DeserializeOwned + Clone + Send + Sync + 'static>(s: &str) -> std::result::Result<T, String> {
    let text = read_json_text(s).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("invalid JSON argument: {e}"))
}
