//!
//! [`AccountKind`] is a unique type identifier of an [`Account`].
//!

use crate::imports::*;
use fixedstr::*;
use std::hash::Hash;
use std::str::FromStr;
use workflow_wasm::convert::CastFromJs;

/// @category Wallet SDK
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Hash, CastFromJs)]
#[wasm_bindgen]
pub struct AccountKind(str64);

#[wasm_bindgen]
impl AccountKind {
    #[wasm_bindgen(constructor)]
    pub fn ctor(kind: &str) -> Result<AccountKind> {
        Self::from_str(kind)
    }
    #[wasm_bindgen(js_name=toString)]
    pub fn js_to_string(&self) -> String {
        self.0.as_str().to_string()
    }
}

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
        Self(kind.into())
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
        if let Ok(kind_ref) = Self::try_ref_from_js_value(&kind) {
            Ok(*kind_ref)
        } else if let Some(kind) = kind.as_string() {
            Ok(AccountKind::from_str(kind.as_str())?)
        } else {
            Err(Error::InvalidAccountKind)
        }
    }
}

impl BorshSerialize for AccountKind {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let len = self.0.len() as u8;
        writer.write_all(&[len])?;
        writer.write_all(self.0.as_bytes())?;
        Ok(())
    }
}

impl BorshDeserialize for AccountKind {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        if buf.is_empty() {
            Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid AccountKind length"))
        } else {
            let len = buf[0];
            if buf.len() < (len as usize + 1) {
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid AccountKind length"))
            } else {
                let s = str64::make(
                    std::str::from_utf8(&buf[1..(len as usize + 1)])
                        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8 sequence"))?,
                );
                *buf = &buf[(len as usize + 1)..];
                Ok(Self(s))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test_storage_account_kind() -> Result<()> {
        let storable_in = AccountKind::from("hello world");
        let guard = StorageGuard::new(&storable_in);
        let storable_out = guard.validate()?;
        assert_eq!(storable_in, storable_out);

        Ok(())
    }
}
