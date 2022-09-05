use thiserror::Error;

use crate::tx::TransactionOutpoint;

#[derive(Error, Debug)]
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

pub type UtxoResult<T> = std::result::Result<T, UtxoAlgebraError>;
