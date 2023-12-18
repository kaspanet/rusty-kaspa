//!
//! [`AccountKind`] is a unique type identifier of an [`Account`].
//! 

use crate::imports::*;
use std::hash::Hash;
use std::str::FromStr;

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Hash)]
#[wasm_bindgen]
pub struct AccountKind(Arc<String>);

impl AccountKind {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for AccountKind {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Display for AccountKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for AccountKind {
    fn from(kind: &str) -> Self {
        Self(kind.to_string().into())
    }
}

impl PartialEq<&str> for AccountKind {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_str() == *other
    }
}

impl FromStr for AccountKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        if factories().contains_key(&s.into()) {
            Ok(s.into())
        } else {
            match s.to_lowercase().as_str() {
                "legacy" => Ok(LEGACY_ACCOUNT_KIND.into()),
                "bip32" => Ok(BIP32_ACCOUNT_KIND.into()),
                "multisig" => Ok(MULTISIG_ACCOUNT_KIND.into()),
                "keypair" => Ok(KEYPAIR_ACCOUNT_KIND.into()),
                _ => Err(Error::InvalidAccountKind),
            }
        }
    }
}

impl TryFrom<JsValue> for AccountKind {
    type Error = Error;
    fn try_from(kind: JsValue) -> Result<Self> {
        if let Some(kind) = kind.as_string() {
            Ok(AccountKind::from_str(kind.as_str())?)
        } else {
            Err(Error::InvalidAccountKind)
        }
    }
}
