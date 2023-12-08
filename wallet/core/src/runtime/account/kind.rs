#[allow(unused_imports)]
use crate::derivation::{gen0::*, gen1::*, PubkeyDerivationManagerTrait, WalletDerivationManagerTrait};
use crate::imports::*;
use crate::result::Result;
use std::hash::Hash;
use std::str::FromStr;
use workflow_core::enums::u8_try_from;

u8_try_from! {
    #[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Hash)]
    #[serde(rename_all = "lowercase")]
    #[wasm_bindgen]
    pub enum AccountKind {
        Legacy,
        #[default]
        Bip32,
        MultiSig,
        Keypair,
        Hardware,
        Resident,
        HTLC,
    }
}

impl std::fmt::Display for AccountKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountKind::Legacy => write!(f, "legacy"),
            AccountKind::Bip32 => write!(f, "bip32"),
            AccountKind::MultiSig => write!(f, "multisig"),
            AccountKind::Keypair => write!(f, "keypair"),
            AccountKind::Hardware => write!(f, "hardware"),
            AccountKind::Resident => write!(f, "resident"),
            AccountKind::HTLC => write!(f, "htlc"),
        }
    }
}

impl FromStr for AccountKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "legacy" => Ok(AccountKind::Legacy),
            "bip32" => Ok(AccountKind::Bip32),
            "multisig" => Ok(AccountKind::MultiSig),
            "keypair" => Ok(AccountKind::Keypair),
            "hardware" => Ok(AccountKind::Hardware),
            "resident" => Ok(AccountKind::Resident),
            "htlc" => Ok(AccountKind::HTLC),
            _ => Err(Error::InvalidAccountKind),
        }
    }
}

impl TryFrom<JsValue> for AccountKind {
    type Error = Error;
    fn try_from(kind: JsValue) -> Result<Self> {
        if let Some(kind) = kind.as_f64() {
            Ok(AccountKind::try_from(kind as u8)?)
        } else if let Some(kind) = kind.as_string() {
            Ok(AccountKind::from_str(kind.as_str())?)
        } else {
            Err(Error::InvalidAccountKind)
        }
    }
}
