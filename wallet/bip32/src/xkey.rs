//! Parser for extended key types (i.e. `xprv` and `xpub`)

use crate::{ChildNumber, Error, ExtendedKeyAttrs, Prefix, Result, Version, KEY_SIZE};
use core::{
    fmt::{self, Display},
    str::{self, FromStr},
};
use zeroize::Zeroize;

/// Serialized extended key (e.g. `xprv` and `xpub`).
#[derive(Clone)]
pub struct ExtendedKey {
    /// [`Prefix`] (a.k.a. "version") of the key (e.g. `xprv`, `xpub`)
    pub prefix: Prefix,

    /// Extended key attributes.
    pub attrs: ExtendedKeyAttrs,

    /// Key material (may be public or private).
    ///
    /// Includes an extra byte for a public key's SEC1 tag.
    pub key_bytes: [u8; KEY_SIZE + 1],
}

impl ExtendedKey {
    /// Size of an extended key when deserialized into bytes from Base58.
    pub const BYTE_SIZE: usize = 78;

    /// Maximum size of a Base58Check-encoded extended key in bytes.
    ///
    /// Note that extended keys can also be 111-bytes.
    pub const MAX_BASE58_SIZE: usize = 112;

    /// Write a Base58-encoded key to the provided buffer, returning a `&str`
    /// containing the serialized data.
    ///
    /// Note that this type also impls [`Display`] and therefore you can
    /// obtain an owned string by calling `to_string()`.
    pub fn write_base58<'a>(&self, buffer: &'a mut [u8; Self::MAX_BASE58_SIZE]) -> Result<&'a str> {
        let mut bytes = [0u8; Self::BYTE_SIZE]; // with 4-byte checksum
        bytes[..4].copy_from_slice(&self.prefix.to_bytes());
        bytes[4] = self.attrs.depth;
        bytes[5..9].copy_from_slice(&self.attrs.parent_fingerprint);
        bytes[9..13].copy_from_slice(&self.attrs.child_number.to_bytes());
        bytes[13..45].copy_from_slice(&self.attrs.chain_code);
        bytes[45..78].copy_from_slice(&self.key_bytes);

        //println!("serialized {}", hex::encode(&bytes));
        let base58_len = bs58::encode(&bytes).with_check().onto(buffer.as_mut())?;
        bytes.zeroize();

        //println!("base58_len: {base58_len}, hash {}", hex::encode(&buffer));

        str::from_utf8(&buffer[..base58_len]).map_err(Error::Utf8Error)
    }
}

impl Display for ExtendedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = [0u8; Self::MAX_BASE58_SIZE];
        self.write_base58(&mut buf).map_err(|_| fmt::Error).and_then(|base58| f.write_str(base58))
    }
}

impl FromStr for ExtendedKey {
    type Err = Error;

    fn from_str(base58: &str) -> Result<Self> {
        let mut bytes = [0u8; Self::BYTE_SIZE + 4]; // with 4-byte checksum
        let decoded_len = bs58::decode(base58).with_check(None).onto(&mut bytes)?;

        if decoded_len != Self::BYTE_SIZE {
            return Err(Error::DecodeLength(decoded_len, Self::BYTE_SIZE));
        }

        let prefix = base58.get(..4).ok_or(Error::DecodeIssue).and_then(|chars| {
            Prefix::validate_str(chars)?;
            let b: [u8; 4] = bytes[..4].try_into()?;
            let version = Version::from_be_bytes(b);
            Ok(Prefix::from_parts_unchecked(chars, version))
            //Err(Error::DecodeIssue)
        })?;

        let depth = bytes[4];
        let parent_fingerprint = bytes[5..9].try_into()?;
        let child_number = ChildNumber::from_bytes(bytes[9..13].try_into()?);
        let chain_code = bytes[13..45].try_into()?;
        let key_bytes = bytes[45..78].try_into()?;
        bytes.zeroize();

        let attrs = ExtendedKeyAttrs { depth, parent_fingerprint, child_number, chain_code };

        Ok(ExtendedKey { prefix, attrs, key_bytes })
    }
}

impl Drop for ExtendedKey {
    fn drop(&mut self) {
        self.key_bytes.zeroize();
    }
}

// TODO(tarcieri): consolidate test vectors
#[cfg(test)]
mod tests {
    use super::ExtendedKey;
    use faster_hex::hex_decode_fallback;

    macro_rules! hex {
        ($str: literal) => {{
            let len = $str.as_bytes().len() / 2;
            let mut dst = vec![0; len];
            dst.resize(len, 0);
            hex_decode_fallback($str.as_bytes(), &mut dst);
            dst
        }
        [..]};
    }

    #[test]
    fn bip32_test_vector_1_xprv() {
        //let xprv_base58 = "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";
        let xprv_base58 = "xprv9s21ZrQH143K3QTDL4LXw2F7HEK3wJUD2nW2nRk4stbPy6cq3jPP\
            qjiChkVvvNKmPGJxWUtg6LnF5kejMRNNU3TGtRBeJgk33yuGBxrMPHi";

        let xprv = xprv_base58.parse::<ExtendedKey>();
        assert!(xprv.is_ok(), "Could not parse key");
        let xprv = xprv.unwrap();
        assert_eq!(xprv.prefix.as_str(), "xprv");
        assert_eq!(xprv.attrs.depth, 0);
        assert_eq!(xprv.attrs.parent_fingerprint, [0u8; 4]);
        assert_eq!(xprv.attrs.child_number.0, 0);
        assert_eq!(xprv.attrs.chain_code, hex!("873DFF81C02F525623FD1FE5167EAC3A55A049DE3D314BB42EE227FFED37D508"));
        assert_eq!(xprv.key_bytes, hex!("00E8F32E723DECF4051AEFAC8E2C93C9C5B214313817CDB01A1494B917C8436B35"));
        assert_eq!(&xprv.to_string(), xprv_base58);
    }

    #[test]
    fn bip32_test_vector_1_xpub() {
        let xpub_base58 = "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhe\
             PY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8";

        let xpub = xpub_base58.parse::<ExtendedKey>();
        assert!(xpub.is_ok(), "Could not parse key");
        let xpub = xpub.unwrap();
        assert_eq!(xpub.prefix.as_str(), "xpub");
        assert_eq!(xpub.attrs.depth, 0);
        assert_eq!(xpub.attrs.parent_fingerprint, [0u8; 4]);
        assert_eq!(xpub.attrs.child_number.0, 0);
        assert_eq!(xpub.attrs.chain_code, hex!("873DFF81C02F525623FD1FE5167EAC3A55A049DE3D314BB42EE227FFED37D508"));
        assert_eq!(xpub.key_bytes, hex!("0339A36013301597DAEF41FBE593A02CC513D0B55527EC2DF1050E2E8FF49C85C2"));
        assert_eq!(&xpub.to_string(), xpub_base58);
    }
}
