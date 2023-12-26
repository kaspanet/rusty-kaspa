//!
//! Private key data info (reference representation).
//!

use crate::imports::*;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PrvKeyDataInfo {
    pub id: PrvKeyDataId,
    pub name: Option<String>,
    pub is_encrypted: bool,
}

impl From<&PrvKeyData> for PrvKeyDataInfo {
    fn from(data: &PrvKeyData) -> Self {
        Self::new(data.id, data.name.clone(), data.payload.is_encrypted())
    }
}

impl PrvKeyDataInfo {
    pub fn new(id: PrvKeyDataId, name: Option<String>, is_encrypted: bool) -> Self {
        Self { id, name, is_encrypted }
    }

    pub fn is_encrypted(&self) -> bool {
        self.is_encrypted
    }

    pub fn name_or_id(&self) -> String {
        if let Some(name) = &self.name {
            name.to_owned()
        } else {
            self.id.to_hex()[0..16].to_string()
        }
    }

    pub fn requires_bip39_passphrase(&self) -> bool {
        self.is_encrypted
    }
}

impl Display for PrvKeyDataInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = &self.name {
            write!(f, "{} ({})", name, self.id.to_hex())?;
        } else {
            write!(f, "{}", self.id.to_hex())?;
        }
        Ok(())
    }
}
