//!
//! Private key data info (reference representation).
//!

use crate::imports::*;
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use std::fmt::{Display, Formatter};

declare! {
    IPrvKeyDataInfo,
    r#"
    /**
     * Private key data information.
     * @category Wallet API
     */
    export interface IPrvKeyDataInfo {
        /** Deterministic wallet id of the private key */
        id: HexString;
        /** Optional name of the private key */
        name?: string;
        /** 
         * Indicates if the key requires additional payment or a recovery secret
         * to perform wallet operations that require access to it.
         * For BIP39 keys this indicates that the key was created with a BIP39 passphrase.
         */
        isEncrypted: boolean;
    }
    "#,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PrvKeyDataInfo {
    pub id: PrvKeyDataId,
    pub name: Option<String>,
    #[serde(rename = "isEncrypted")]
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
