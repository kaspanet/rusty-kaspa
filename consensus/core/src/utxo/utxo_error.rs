use thiserror::Error;

use crate::tx::TransactionOutpoint;

#[derive(Error, Debug, Eq)]
pub enum UtxoAlgebraError {
    #[error("outpoint {0} both in self.remove and in other.remove")]
    DuplicateRemovePoint(TransactionOutpoint),

    #[error("outpoint {0} both in self.add and in other.add")]
    DuplicateAddPoint(TransactionOutpoint),

    #[error("cannot remove outpoint {0} twice")]
    DoubleRemoveCall(TransactionOutpoint),

    #[error("cannot add outpoint {0} twice")]
    DoubleAddCall(TransactionOutpoint),

    #[error("outpoint {0} {1}")]
    DiffIntersectionPoint(TransactionOutpoint, &'static str),

    #[error("{0}")]
    General(&'static str),
}

/// Explicit imp in order to ignore the description strings in test equality assertions
impl PartialEq for UtxoAlgebraError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::DuplicateRemovePoint(l0), Self::DuplicateRemovePoint(r0)) => l0 == r0,
            (Self::DuplicateAddPoint(l0), Self::DuplicateAddPoint(r0)) => l0 == r0,
            (Self::DoubleRemoveCall(l0), Self::DoubleRemoveCall(r0)) => l0 == r0,
            (Self::DoubleAddCall(l0), Self::DoubleAddCall(r0)) => l0 == r0,
            (Self::DiffIntersectionPoint(l0, _), Self::DiffIntersectionPoint(r0, _)) => l0 == r0, // Ignore the description string
            (Self::General(_), Self::General(_)) => true,
            (_, _) => false,
        }
    }
}

pub type UtxoResult<T> = std::result::Result<T, UtxoAlgebraError>;
