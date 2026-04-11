use crate::zk_precompiles::error::ZkIntegrityError;

pub type Result<T> = std::result::Result<T, ZkIntegrityError>;