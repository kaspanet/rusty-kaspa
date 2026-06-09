use crate::zk_precompiles::risc0::R0Error;

pub type Result<T> = std::result::Result<T, R0Error>;
