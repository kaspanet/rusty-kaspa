//! Configuration loading and precedence.
//!
//! Effective settings are resolved with precedence: CLI flags > environment
//! (`KASPA_RPC_*`) > config file (`~/.config/kaspa-rpc/config.toml`).
//!
//! This module is responsible only for the file + environment layers; CLI
//! flags are layered on top by the caller in `lib.rs`.

use crate::cli::EncodingArg;
use crate::error::{CliError, Result};
use crate::output::OutputFormat;
use crate::transport::Transport;
use kaspa_consensus_core::network::NetworkId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// File/environment-derived configuration. All fields are optional; unset
/// fields fall through to the next-lower precedence layer or a built-in default.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct Config {
    pub url: Option<String>,
    /// Network id (e.g. `mainnet`, `testnet-10`); parsed via its string form.
    #[serde(with = "serde_network", default)]
    pub network: Option<NetworkId>,
    pub transport: Option<Transport>,
    pub encoding: Option<EncodingArg>,
    pub output: Option<OutputFormat>,
    pub timeout: Option<u64>,
}

/// Serde adapter for `Option<NetworkId>` using its `Display`/`FromStr` string
/// form (e.g. `network = "testnet-10"`), since the derived serde is struct-form.
mod serde_network {
    use super::{FromStr, NetworkId};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Option<NetworkId>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(id) => serializer.serialize_some(&id.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<NetworkId>, D::Error> {
        let opt = Option::<String>::deserialize(deserializer)?;
        match opt {
            Some(s) => NetworkId::from_str(&s).map(Some).map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

impl Config {
    /// Default config file path: `~/.config/kaspa-rpc/config.toml` (XDG).
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("kaspa-rpc").join("config.toml"))
    }

    /// Settable keys, in file/display order. Shared by `config set`/`unset`, the
    /// environment overlay, and key-validation error messages.
    pub const KEYS: &'static [&'static str] = &["url", "network", "transport", "encoding", "output", "timeout"];

    /// Load and merge the file and environment layers.
    ///
    /// - When `no_config` is set, the file layer is skipped entirely.
    /// - When `explicit` is provided, that path is required to exist.
    /// - Otherwise the default path is loaded if present (absence is not an error).
    pub fn load(explicit: Option<&Path>, no_config: bool) -> Result<Config> {
        let mut cfg = if no_config {
            Config::default()
        } else if let Some(path) = explicit {
            Self::from_file(path)?
        } else if let Some(path) = Self::default_path() {
            Self::from_file_or_default(&path)?
        } else {
            Config::default()
        };

        cfg.apply_env()?;
        Ok(cfg)
    }

    fn from_file(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path).map_err(|e| CliError::Config(format!("reading {}: {e}", path.display())))?;
        Ok(toml::from_str(&text)?)
    }

    /// Load `path`, or an empty default config when the file is absent. Used by
    /// the `config set`/`unset` editors, which must not fail on a missing file.
    pub fn from_file_or_default(path: &Path) -> Result<Config> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(CliError::Config(format!("reading {}: {e}", path.display()))),
        }
    }

    /// Serialize to TOML and write to `path`, creating parent directories. Unset
    /// (`None`) fields are omitted from the file.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CliError::Config(format!("creating {}: {e}", parent.display())))?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, &text).map_err(|e| CliError::Config(format!("writing {}: {e}", path.display())))?;
        Ok(())
    }

    /// Set (`Some`) or clear (`None`) one field by its canonical key name,
    /// validating the value against the field's type. Shared by `config
    /// set`/`unset` and the environment overlay.
    pub fn set_field(&mut self, key: &str, value: Option<&str>) -> Result<()> {
        match key {
            "url" => self.url = value.map(str::to_string),
            "network" => {
                self.network = parse_opt(value, |v| NetworkId::from_str(v).map_err(|e| format!("invalid network '{v}': {e}")))?
            }
            "transport" => {
                self.transport = parse_opt(value, |v| match v {
                    "grpc" => Ok(Transport::Grpc),
                    "wrpc" => Ok(Transport::Wrpc),
                    other => Err(format!("invalid transport '{other}'")),
                })?
            }
            "encoding" => {
                self.encoding = parse_opt(value, |v| match v {
                    "borsh" => Ok(EncodingArg::Borsh),
                    "json" => Ok(EncodingArg::Json),
                    other => Err(format!("invalid encoding '{other}'")),
                })?
            }
            "output" => {
                self.output = parse_opt(value, |v| match v {
                    "json" => Ok(OutputFormat::Json),
                    "text" => Ok(OutputFormat::Text),
                    other => Err(format!("invalid output '{other}'")),
                })?
            }
            "timeout" => self.timeout = parse_opt(value, |v| v.parse::<u64>().map_err(|e| format!("invalid timeout '{v}': {e}")))?,
            other => return Err(CliError::Config(format!("unknown config key '{other}'; valid keys: {}", Self::KEYS.join(", ")))),
        }
        Ok(())
    }

    /// Overlay `KASPA_RPC_*` environment variables (higher precedence than file).
    fn apply_env(&mut self) -> Result<()> {
        for &key in Self::KEYS {
            if let Ok(v) = std::env::var(format!("KASPA_RPC_{}", key.to_uppercase())) {
                self.set_field(key, Some(&v))?;
            }
        }
        Ok(())
    }
}

/// Apply `f` to a present value, mapping a parse failure into `CliError::Config`;
/// `None` (unset) passes through unchanged.
fn parse_opt<T>(value: Option<&str>, f: impl FnOnce(&str) -> std::result::Result<T, String>) -> Result<Option<T>> {
    value.map(f).transpose().map_err(CliError::Config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `set_field` accepts every key and the value survives a TOML round-trip.
    #[test]
    fn set_field_round_trips_every_key() {
        let mut cfg = Config::default();
        for (key, value) in [
            ("url", "ws://node:17110"),
            ("network", "testnet-10"),
            ("transport", "grpc"),
            ("encoding", "json"),
            ("output", "json"),
            ("timeout", "45"),
        ] {
            cfg.set_field(key, Some(value)).unwrap();
        }
        let reparsed: Config = toml::from_str(&toml::to_string_pretty(&cfg).unwrap()).unwrap();
        assert_eq!(reparsed.url.as_deref(), Some("ws://node:17110"));
        assert_eq!(reparsed.network.map(|n| n.to_string()).as_deref(), Some("testnet-10"));
        assert_eq!(reparsed.transport, Some(Transport::Grpc));
        assert_eq!(reparsed.encoding, Some(EncodingArg::Json));
        assert_eq!(reparsed.output, Some(OutputFormat::Json));
        assert_eq!(reparsed.timeout, Some(45));
    }

    /// Unset clears a previously-set field, which then drops out of the file.
    #[test]
    fn unset_clears_field() {
        let mut cfg = Config::default();
        cfg.set_field("url", Some("ws://node")).unwrap();
        cfg.set_field("url", None).unwrap();
        assert!(cfg.url.is_none());
        assert!(!toml::to_string_pretty(&cfg).unwrap().contains("url"));
    }

    /// Unknown keys and malformed values are rejected.
    #[test]
    fn set_field_rejects_bad_input() {
        let mut cfg = Config::default();
        assert!(cfg.set_field("nope", Some("x")).is_err());
        assert!(cfg.set_field("timeout", Some("soon")).is_err());
        assert!(cfg.set_field("network", Some("notanet")).is_err());
        assert!(cfg.set_field("transport", Some("carrier-pigeon")).is_err());
    }
}
