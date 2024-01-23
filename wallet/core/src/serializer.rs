//!
//! Helpers for binary serialization and deserialization used by the storage subsystem.
//!

use crate::imports::*;

/// A struct that contains the magic `u32` and the version `u32` values of the serialized data.
/// This struct is used by various primitives in the wallet framework to serialize their
/// versions and detect these versions when deserializing. This allows for future
/// data storage compatibility of the wallet subsystem, even if serialized structures themselves
/// change. The `magic` value is a unique `u32` value set by each structure and used to detect
/// memory alignment errors during deserialization.
#[derive(Debug, Clone)]
pub struct StorageHeader {
    pub magic: u32,
    pub version: u32,
}

impl StorageHeader {
    pub fn new(magic: u32, version: u32) -> Self {
        Self { magic, version }
    }

    pub fn try_magic(self, magic: u32) -> IoResult<Self> {
        if self.magic != magic {
            Err(IoError::new(
                IoErrorKind::Other,
                format!(
                    "Deserializer magic value error: expected '0x{}' received '0x{}'",
                    magic.to_le_bytes().as_slice().to_hex(),
                    self.magic.to_le_bytes().as_slice().to_hex()
                ),
            ))
        } else {
            Ok(self)
        }
    }

    pub fn try_version(self, version: u32) -> IoResult<Self> {
        if self.version > version {
            Err(IoError::new(
                IoErrorKind::Other,
                format!(
                    "Deserializer data has a newer version than the current version: expected version at most '{}' received '{}' (your data may have been generated on a newer version of the software)",
                    version,
                    self.version
                ),
            ))
        } else {
            Ok(self)
        }
    }
}

impl BorshSerialize for StorageHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&(self.magic, self.version), writer)
    }
}

impl BorshDeserialize for StorageHeader {
    fn deserialize(buf: &mut &[u8]) -> std::io::Result<Self> {
        let (magic, version): (u32, u32) = BorshDeserialize::deserialize(buf)?;
        Ok(Self { magic, version })
    }
}

pub type IoError = std::io::Error;
pub type IoErrorKind = std::io::ErrorKind;
pub type IoResult<T> = std::io::Result<T>;
