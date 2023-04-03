pub use super::error::Error;
use crate::tx::SignableTransaction;

pub type Result = std::result::Result<SignableTransaction, super::error::Error>;

pub trait Signer {
    fn sign(&self, _mtx: SignableTransaction) -> Result;
}
