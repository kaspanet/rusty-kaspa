use crate::RpcError;
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

// TODO: in the future it should be a newtype of ScriptClass, that will be probably a type
// associated with the script engine
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum RpcScriptClass {
    /// None of the recognized forms.
    NonStandardTy = 0,

    /// Pay to pubkey.
    PubKeyTy = 1,

    /// Pay to pubkey ECDSA.
    PubKeyECDSATy = 2,

    /// Pay to script hash.
    ScriptHashTy = 3,
}

const NON_STANDARD_TY: &str = "nonstandard";
const PUB_KEY_TY: &str = "pubkey";
const PUB_KEY_ECDSA_TY: &str = "pubkeyecdsa";
const SCRIPT_HASH_TY: &str = "scripthash";

impl RpcScriptClass {
    fn as_str(&self) -> &'static str {
        match self {
            RpcScriptClass::NonStandardTy => NON_STANDARD_TY,
            RpcScriptClass::PubKeyTy => PUB_KEY_TY,
            RpcScriptClass::PubKeyECDSATy => PUB_KEY_ECDSA_TY,
            RpcScriptClass::ScriptHashTy => SCRIPT_HASH_TY,
        }
    }
}

impl Display for RpcScriptClass {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RpcScriptClass {
    type Err = RpcError;

    fn from_str(script_class: &str) -> Result<Self, Self::Err> {
        match script_class {
            NON_STANDARD_TY => Ok(RpcScriptClass::NonStandardTy),
            PUB_KEY_TY => Ok(RpcScriptClass::PubKeyTy),
            PUB_KEY_ECDSA_TY => Ok(RpcScriptClass::PubKeyECDSATy),
            SCRIPT_HASH_TY => Ok(RpcScriptClass::ScriptHashTy),

            _ => Err(RpcError::InvalidRpcScriptClass(script_class.to_string())),
        }
    }
}

impl TryFrom<&str> for RpcScriptClass {
    type Error = RpcError;

    fn try_from(script_class: &str) -> Result<Self, Self::Error> {
        script_class.parse()
    }
}
