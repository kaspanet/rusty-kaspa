//!
//! User hint is a string that can be stored in the wallet
//! and presented to the user when the wallet opens to help
//! prevent phishing attacks.
//!

use crate::imports::*;
use borsh::{BorshDeserialize, BorshSerialize};
use std::fmt::{Display, Formatter};

#[derive(Default, Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Hint {
    pub text: String,
}

impl Hint {
    pub fn new(text: String) -> Self {
        Self { text }
    }
}

impl From<&str> for Hint {
    fn from(text: &str) -> Self {
        Self::new(text.to_string())
    }
}

impl From<String> for Hint {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl Display for Hint {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}
