use crate::imports::*;
use std::fmt::{Display, Formatter};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Hint {
    pub text: String,
}

impl Hint {
    pub fn new(text: String) -> Self {
        Self { text }
    }
}

impl Display for Hint {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}
