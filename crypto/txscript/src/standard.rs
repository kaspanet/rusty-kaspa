use addresses::{Address, Prefix, Version};
use consensus_core::tx::{ScriptPublicKey, ScriptVec};
use smallvec::SmallVec;
use std::iter::once;
use txscript_errors::TxScriptError;

use crate::{
    opcodes::codes::{OpBlake2b, OpCheckSig, OpCheckSigECDSA, OpData32, OpData33, OpEqual},
    script_class::ScriptClass,
};

/// Creates a new script to pay a transaction output to a 32-byte pubkey.
fn pay_to_pub_key_script(address_payload: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(address_payload.len(), 32);
    SmallVec::from_iter(once(OpData32).chain(address_payload.iter().copied()).chain(once(OpCheckSig)))
}

/// Creates a new script to pay a transaction output to a 33-byte ECDSA pubkey.
fn pay_to_pub_key_ecdsa_script(address_payload: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(address_payload.len(), 33);
    SmallVec::from_iter(once(OpData33).chain(address_payload.iter().copied()).chain(once(OpCheckSigECDSA)))
}

// Creates a new script to pay a transaction output to a script hash.
// It is expected that the input is a valid hash.
fn pay_to_script_hash_script(script_hash: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(script_hash.len(), 32);
    SmallVec::from_iter([OpBlake2b, OpData32].iter().copied().chain(script_hash.iter().copied()).chain(once(OpEqual)))
}

/// Creates a new script to pay a transaction output to the specified address.
pub fn pay_to_address_script(address: &Address) -> ScriptPublicKey {
    let script = match address.version {
        Version::PubKey => pay_to_pub_key_script(address.payload.as_slice()),
        Version::PubKeyECDSA => pay_to_pub_key_ecdsa_script(address.payload.as_slice()),
        Version::ScriptHash => pay_to_script_hash_script(address.payload.as_slice()),
    };
    ScriptPublicKey::new(ScriptClass::from(address.version).version(), script)
}

/// Returns the address encoded in a script public key.
///
/// Notes:
///  - This function only works for 'standard' transaction script types.
///    Any data such as public keys which are invalid will return the
///    `TxScriptError::PubKeyFormat` error.
///
///  - In case a ScriptClass is needed by the caller, call `ScriptClass::from(address.version)`
///    or use `address.version` directly instead, where address is the successfully
///    returned address.
pub fn extract_script_pub_key_address(script_public_key: &ScriptPublicKey, prefix: Prefix) -> Result<Address, TxScriptError> {
    let script = script_public_key.script();
    let class = ScriptClass::from_script(script);
    if script_public_key.version() > class.version() {
        return Err(TxScriptError::PubKeyFormat);
    }
    match class {
        ScriptClass::NonStandard => Err(TxScriptError::PubKeyFormat),
        ScriptClass::PubKey => Ok(Address::new(prefix, Version::PubKey, &script[1..33])),
        ScriptClass::PubKeyECDSA => Ok(Address::new(prefix, Version::PubKeyECDSA, &script[1..34])),
        ScriptClass::ScriptHash => Ok(Address::new(prefix, Version::ScriptHash, &script[2..34])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_address_and_encode_script() {
        struct Test {
            name: &'static str,
            script_pub_key: ScriptPublicKey,
            prefix: Prefix,
            expected_address: Result<Address, TxScriptError>,
        }

        // cspell:disable
        let tests = vec![
            Test {
                name: "Mainnet PubKey script and address",
                script_pub_key: ScriptPublicKey::new(
                    ScriptClass::PubKey.version(),
                    ScriptVec::from_slice(
                        &hex::decode("207bc04196f1125e4f2676cd09ed14afb77223b1f62177da5488346323eaa91a69ac").unwrap(),
                    ),
                ),
                prefix: Prefix::Mainnet,
                expected_address: Ok("kaspa:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j".try_into().unwrap()),
            },
            Test {
                name: "Testnet PubKeyECDSA script and address",
                script_pub_key: ScriptPublicKey::new(
                    ScriptClass::PubKeyECDSA.version(),
                    ScriptVec::from_slice(
                        &hex::decode("21ba01fc5f4e9d9879599c69a3dafdb835a7255e5f2e934e9322ecd3af190ab0f60eab").unwrap(),
                    ),
                ),
                prefix: Prefix::Testnet,
                expected_address: Ok("kaspatest:qxaqrlzlf6wes72en3568khahq66wf27tuhfxn5nytkd8tcep2c0vrse6gdmpks".try_into().unwrap()),
            },
            Test {
                name: "Testnet non standard script",
                script_pub_key: ScriptPublicKey::new(
                    ScriptClass::PubKey.version(),
                    ScriptVec::from_slice(
                        &hex::decode("2001fc5f4e9d9879599c69a3dafdb835a7255e5f2e934e9322ecd3af190ab0f60eab").unwrap(),
                    ),
                ),
                prefix: Prefix::Testnet,
                expected_address: Err(TxScriptError::PubKeyFormat),
            },
            Test {
                name: "Mainnet script with unknown version",
                script_pub_key: ScriptPublicKey::new(
                    ScriptClass::PubKey.version() + 1,
                    ScriptVec::from_slice(
                        &hex::decode("207bc04196f1125e4f2676cd09ed14afb77223b1f62177da5488346323eaa91a69ac").unwrap(),
                    ),
                ),
                prefix: Prefix::Mainnet,
                expected_address: Err(TxScriptError::PubKeyFormat),
            },
        ];
        // cspell:enable

        for test in tests {
            let extracted = extract_script_pub_key_address(&test.script_pub_key, test.prefix);
            assert_eq!(extracted, test.expected_address, "extract address test failed for '{}'", test.name);
            if let Ok(ref address) = extracted {
                let encoded = pay_to_address_script(address);
                assert_eq!(encoded, test.script_pub_key, "encode public key script test failed for '{}'", test.name);
            }
        }
    }
}
