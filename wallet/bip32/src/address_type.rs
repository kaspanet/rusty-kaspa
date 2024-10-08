//!
//!  Address type (`Receive` or `Change`) used in HD wallet address derivation.
//!

use std::fmt;

/// Address type used in HD wallet address derivation.
pub enum AddressType {
    Receive = 0,
    Change,
}

impl fmt::Display for AddressType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Receive => write!(f, "Receive"),
            Self::Change => write!(f, "Change"),
        }
    }
}

impl AddressType {
    pub fn index(&self) -> u32 {
        match self {
            Self::Receive => 0,
            Self::Change => 1,
        }
    }
}
