//! Extended key prefixes.

use crate::{Error, ExtendedKey, Result, Version};
use borsh::{BorshDeserialize, BorshSerialize};
use core::{
    fmt::{self, Debug, Display},
    str,
};

/// BIP32 extended key prefixes a.k.a. "versions" (e.g. `xpub`, `xprv`)
///
/// The BIP32 spec describes these as "versions" and gives examples for
/// `xprv`/`xpub` (mainnet) and `tprv`/`tpub` (testnet), however in practice
/// there are many more used (e.g. `ypub`, `zpub`).
#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[non_exhaustive]
pub struct Prefix {
    /// ASCII characters representing the prefix.
    chars: [u8; Self::LENGTH],

    /// Prefix after Base58 decoding, interpreted as a big endian integer.
    version: Version,
}

impl Prefix {
    /// Length of a prefix in ASCII characters.
    pub const LENGTH: usize = 4;

    /// `kprv` prefix
    pub const KPRV: Self = Self::from_parts_unchecked("kprv", 0x038f2ef4);

    /// `kpub` prefix
    pub const KPUB: Self = Self::from_parts_unchecked("kpub", 0x038f332e);

    /// `ktrv` prefix
    pub const KTRV: Self = Self::from_parts_unchecked("ktrv", 0x03909e07);

    /// `ktub` prefix
    pub const KTUB: Self = Self::from_parts_unchecked("ktub", 0x0390a241);

    /// `tprv` prefix
    pub const TPRV: Self = Self::from_parts_unchecked("tprv", 0x04358394);

    /// `tpub` prefix
    pub const TPUB: Self = Self::from_parts_unchecked("tpub", 0x043587cf);

    /// `xprv` prefix
    pub const XPRV: Self = Self::from_parts_unchecked("xprv", 0x0488ade4);

    /// `xpub` prefix
    pub const XPUB: Self = Self::from_parts_unchecked("xpub", 0x0488b21e);

    /// `yprv` prefix
    pub const YPRV: Self = Self::from_parts_unchecked("yprv", 0x049d7878);

    /// `ypub` prefix
    pub const YPUB: Self = Self::from_parts_unchecked("ypub", 0x049d7cb2);

    /// `zprv` prefix
    pub const ZPRV: Self = Self::from_parts_unchecked("zprv", 0x04b2430c);

    /// `zpub` prefix
    pub const ZPUB: Self = Self::from_parts_unchecked("zpub", 0x04b24746);

    /// Create a new prefix from the given 4-character string and version number.
    /// The main intended use case for this function is [`Prefix`] constants
    /// such as [`Prefix::XPRV`].
    ///
    /// # Warning
    ///
    /// Use this function with care: No consistency check is performed! It is
    /// up to the caller to ensure that the version number matches the prefix.
    ///
    /// # Panics
    ///
    /// Panics if `s` is not 4 chars long, or any of the chars lie outside of
    /// the supported range: lower case (`a..=z`) or upper case (`A..=Z`)
    /// letters.
    pub const fn from_parts_unchecked(s: &str, version: Version) -> Self {
        assert!(Self::validate_str(s).is_ok(), "invalid prefix");
        let bytes = s.as_bytes();
        let chars = [bytes[0], bytes[1], bytes[2], bytes[3]];
        Self { chars, version }
    }

    /// Create a new prefix from the given encoded bytes.
    ///
    /// These bytes represent the big endian serialization of a [`Version`] integer.
    pub fn from_bytes(bytes: [u8; Self::LENGTH]) -> Result<Self> {
        Self::from_version(Version::from_be_bytes(bytes))
    }

    /// Parse a [`Prefix`] from a 32-bit integer "version", e.g.:
    ///
    /// - 0x0488B21E => `xpub`
    /// - 0x0488ADE4 => `xprv`
    fn from_version(version: Version) -> Result<Self> {
        let mut bytes = [0u8; ExtendedKey::BYTE_SIZE];
        bytes[..4].copy_from_slice(&version.to_be_bytes());

        let mut buffer = [0u8; ExtendedKey::MAX_BASE58_SIZE];
        bs58::encode(&bytes).with_check().onto(buffer.as_mut())?;

        let s = str::from_utf8(&buffer[..4]).map_err(Error::Utf8Error)?;
        Self::validate_str(s)?;
        Ok(Self::from_parts_unchecked(s, version))
    }

    /// Get the prefix as a string.
    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.chars).expect("prefix encoding error")
    }

    /// Is this a public key?
    pub fn is_public(self) -> bool {
        &self.chars[1..] == b"pub" || &self.chars[1..] == b"tub"
    }

    /// Is this a private key?
    pub fn is_private(self) -> bool {
        &self.chars[1..] == b"prv" || &self.chars[1..] == b"trv"
    }

    /// Get the [`Version`] number.
    pub fn version(self) -> Version {
        self.version
    }

    /// Serialize the [`Version`] number as big-endian bytes.
    pub fn to_bytes(self) -> [u8; Self::LENGTH] {
        self.version.to_be_bytes()
    }

    /// Validate that the given prefix string is well-formed.
    // TODO(tarcieri): validate the string ends with `prv` or `pub`?
    pub(crate) const fn validate_str(s: &str) -> crate::error::ResultConst<&str> {
        if s.as_bytes().len() != Self::LENGTH {
            return Err(crate::error::ErrorImpl::DecodeInvalidLength);
        }

        let mut i = 0;

        while i < Self::LENGTH {
            if s.as_bytes()[i].is_ascii_alphabetic() {
                i += 1;
            } else {
                return Err(crate::error::ErrorImpl::DecodeInvalidStr);
            }
        }

        Ok(s)
    }
}

impl AsRef<str> for Prefix {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Debug for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Prefix").field("chars", &self.as_str()).field("version", &DebugVersion(self.version)).finish()
    }
}

impl Display for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<Prefix> for Version {
    fn from(prefix: Prefix) -> Version {
        prefix.version()
    }
}

impl From<&Prefix> for Version {
    fn from(prefix: &Prefix) -> Version {
        prefix.version()
    }
}

impl TryFrom<Version> for Prefix {
    type Error = Error;

    fn try_from(version: Version) -> Result<Self> {
        Self::from_version(version)
    }
}

impl TryFrom<&[u8]> for Prefix {
    type Error = Error;

    fn try_from(bytes: &[u8]) -> Result<Prefix> {
        Self::from_bytes(bytes.try_into()?)
    }
}

/// Debugging formatting helper for [`Version`] with a `Debug` impl that
/// outputs hexadecimal instead of base 10.
struct DebugVersion(Version);

impl Debug for DebugVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#010x}", self.0)
    }
}

impl TryFrom<&str> for Prefix {
    type Error = Error;
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match value {
            "kprv" => Ok(Prefix::KPRV),
            "kpub" => Ok(Prefix::KPUB),

            "ktrv" => Ok(Prefix::KTRV),
            "ktub" => Ok(Prefix::KTUB),

            "tprv" => Ok(Prefix::TPRV),
            "tpub" => Ok(Prefix::TPUB),

            "xprv" => Ok(Prefix::XPRV),
            "xpub" => Ok(Prefix::XPUB),

            "yprv" => Ok(Prefix::YPRV),
            "ypub" => Ok(Prefix::YPUB),

            "zprv" => Ok(Prefix::ZPRV),
            "zpub" => Ok(Prefix::ZPUB),
            _ => Err(Error::String(format!("Invalid prefix: {value}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Prefix;

    #[test]
    fn constants() {
        assert_eq!(Prefix::TPRV, Prefix::try_from(0x04358394).unwrap());
        assert_eq!(Prefix::TPRV.as_str(), "tprv");
        assert_eq!(Prefix::TPUB, Prefix::try_from(0x043587CF).unwrap());
        assert_eq!(Prefix::TPUB.as_str(), "tpub");

        assert_eq!(Prefix::XPRV, Prefix::try_from(0x0488ADE4).unwrap());
        assert_eq!(Prefix::XPRV.as_str(), "xprv");
        assert_eq!(Prefix::XPUB, Prefix::try_from(0x0488B21E).unwrap());
        assert_eq!(Prefix::XPUB.as_str(), "xpub");

        assert_eq!(Prefix::YPRV, Prefix::try_from(0x049d7878).unwrap());
        assert_eq!(Prefix::YPRV.as_str(), "yprv");
        assert_eq!(Prefix::YPUB, Prefix::try_from(0x049d7cb2).unwrap());
        assert_eq!(Prefix::YPUB.as_str(), "ypub");

        assert_eq!(Prefix::ZPRV, Prefix::try_from(0x04b2430c).unwrap());
        assert_eq!(Prefix::ZPRV.as_str(), "zprv");
        assert_eq!(Prefix::ZPUB, Prefix::try_from(0x04b24746).unwrap());
        assert_eq!(Prefix::ZPUB.as_str(), "zpub");
    }
}
