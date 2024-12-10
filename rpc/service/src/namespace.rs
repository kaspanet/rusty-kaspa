use serde::Deserialize;
use std::collections::HashSet;
use std::str::FromStr;
use thiserror::Error;

/// Enum representing available namespace groups
#[derive(Debug, Hash, Eq, PartialEq, Clone, Deserialize)]
pub enum Namespace {
    General,
    Networking,
    DAG,
    Mining,
    Wallet,
    Metrics,
    Mempool,
}

#[derive(Debug, Error)]
pub enum NamespaceError {
    #[error("Invalid namespace value: {0}")]
    InvalidValue(String),
}

impl FromStr for Namespace {
    type Err = NamespaceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "General" => Ok(Namespace::General),
            "Networking" => Ok(Namespace::Networking),
            "DAG" => Ok(Namespace::DAG),
            "Mining" => Ok(Namespace::Mining),
            "Wallet" => Ok(Namespace::Wallet),
            "Metrics" => Ok(Namespace::Metrics),
            "Mempool" => Ok(Namespace::Mempool),
            _ => Err(NamespaceError::InvalidValue(s.to_string())),
        }
    }
}

impl std::fmt::Display for Namespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Namespace::General => write!(f, "General"),
            Namespace::Networking => write!(f, "Networking"),
            Namespace::DAG => write!(f, "DAG"),
            Namespace::Mining => write!(f, "Mining"),
            Namespace::Wallet => write!(f, "Wallet"),
            Namespace::Metrics => write!(f, "Metrics"),
            Namespace::Mempool => write!(f, "Mempool"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Namespaces {
    enabled: HashSet<Namespace>,
}

impl Namespaces {
    /// Check if a namespace is enabled
    pub fn is_enabled(&self, namespace: &Namespace) -> bool {
        self.enabled.contains(namespace)
    }
}

impl FromStr for Namespaces {
    type Err = NamespaceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let enabled = s
            .split(',')
            .map(str::trim) // To support case like "DAG, Metrics"
            .map(|name| name.parse::<Namespace>())
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(Namespaces { enabled })
    }
}

impl Default for Namespaces {
    fn default() -> Self {
        Self { enabled: HashSet::from([Namespace::General, Namespace::DAG, Namespace::Wallet]) }
    }
}
