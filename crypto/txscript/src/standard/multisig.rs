use crate::opcodes::codes::{OpCheckMultiSig, OpCheckMultiSigECDSA};
use crate::script_builder::{ScriptBuilder, ScriptBuilderError};
use kaspa_addresses::{Address, Version};
use std::borrow::Borrow;
use thiserror::Error;

#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum Error {
    // ErrTooManyRequiredSigs is returned from multisig_script when the
    // specified number of required signatures is larger than the number of
    // provided public keys.
    #[error("too many required signatures")]
    ErrTooManyRequiredSigs,
    #[error(transparent)]
    ScriptBuilderError(#[from] ScriptBuilderError),
    #[error("public key address version should be the same for all provided keys")]
    WrongVersion,
    #[error("provided public keys should not be empty")]
    EmptyKeys,
}

/// Generates a multi-signature redeem script from sorted public keys.
///
/// This function builds a redeem script requiring `required` out of the
/// already sorted `pub_keys` given. It is expected that the public keys
/// are provided in a sorted order.
///
/// # Parameters
///
/// * `pub_keys`: An iterator over sorted public key addresses.
/// * `required`: The number of required signatures to spend the funds.
///
/// # Returns
///
/// A `Result` containing the redeem script in the form of a `Vec<u8>`
/// or an error of type `Error`.
///
/// # Errors
///
/// This function will return an error if:
/// * The number of provided keys is less than `required`.
/// * The public keys contain an unexpected version.
/// * There are no public keys provided.
pub fn multisig_redeem_script_sorted(
    mut pub_keys: impl Iterator<Item = impl Borrow<Address>>,
    required: usize,
) -> Result<Vec<u8>, Error> {
    if pub_keys.size_hint().1.is_some_and(|upper| upper < required) {
        return Err(Error::ErrTooManyRequiredSigs);
    };
    let mut builder = ScriptBuilder::new();
    builder.add_i64(required as i64)?;

    let mut count = 0i64;
    let version = match pub_keys.next() {
        None => return Err(Error::EmptyKeys),
        Some(pub_key) => {
            count += 1;
            builder.add_data(pub_key.borrow().payload.as_slice())?;
            match pub_key.borrow().version {
                v @ Version::PubKey => v,
                v @ Version::PubKeyECDSA => v,
                Version::ScriptHash => {
                    return Err(Error::WrongVersion); // todo is it correct?
                }
            }
        }
    };

    for pub_key in pub_keys {
        if pub_key.borrow().version != version {
            return Err(Error::WrongVersion);
        }
        count += 1;
        builder.add_data(pub_key.borrow().payload.as_slice())?;
    }
    if (count as usize) < required {
        return Err(Error::ErrTooManyRequiredSigs);
    }

    builder.add_i64(count)?;
    if version == Version::PubKeyECDSA {
        builder.add_op(OpCheckMultiSigECDSA)?;
    } else {
        builder.add_op(OpCheckMultiSig)?;
    }

    Ok(builder.drain())
}

///
/// This function sorts the provided public keys and then constructs
/// a redeem script requiring `required` out of the sorted keys.
///
/// # Parameters
///
/// * `pub_keys`: A mutable slice of public key addresses. The keys can be in any order.
/// * `required`: The number of required signatures to spend the funds.
///
/// # Returns
///
/// A `Result` containing the redeem script in the form of a `Vec<u8>`
/// or an error of type `Error`.
///
/// # Errors
///
/// This function will return an error if:
/// * The number of provided keys is less than `required`.
/// * The public keys contain an unexpected version.
/// * There are no public keys provided.
pub fn multisig_redeem_script(pub_keys: &mut [Address], required: usize) -> Result<Vec<u8>, Error> {
    pub_keys.sort();
    multisig_redeem_script_sorted(pub_keys.iter(), required)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{caches::Cache, opcodes::codes::OpData65, pay_to_address_script, pay_to_script_hash_script, TxScriptEngine};
    use core::str::FromStr;
    use kaspa_addresses::Prefix;
    use kaspa_consensus_core::hashing::sighash::calc_ecdsa_signature_hash;
    use kaspa_consensus_core::{
        hashing::sighash::{calc_schnorr_signature_hash, SigHashReusedValues},
        hashing::sighash_type::SIG_HASH_ALL,
        subnets::SubnetworkId,
        tx::*,
    };
    use rand::thread_rng;
    use secp256k1::KeyPair;
    use std::iter;
    use std::iter::empty;

    struct Input {
        kp: KeyPair,
        required: bool,
        sign: bool,
    }

    fn kp() -> [KeyPair; 3] {
        let kp1 = KeyPair::from_seckey_slice(
            secp256k1::SECP256K1,
            hex::decode("1d99c236b1f37b3b845336e6c568ba37e9ced4769d83b7a096eec446b940d160").unwrap().as_slice(),
        )
        .unwrap();
        let kp2 = KeyPair::from_seckey_slice(
            secp256k1::SECP256K1,
            hex::decode("349ca0c824948fed8c2c568ce205e9d9be4468ef099cad76e3e5ec918954aca4").unwrap().as_slice(),
        )
        .unwrap();
        let kp3 = KeyPair::new(secp256k1::SECP256K1, &mut thread_rng());
        [kp1, kp2, kp3]
    }

    #[test]
    fn test_too_many_required_sigs() {
        let payload = vec![0u8; 32];
        let address1 = Address::new(Prefix::Testnet, Version::PubKey, &payload);
        let address2 = Address::new(Prefix::Testnet, Version::PubKey, &payload);
        let pub_keys = vec![&address1, &address2];
        let result = multisig_redeem_script_sorted(pub_keys.into_iter(), 3);
        assert_eq!(result, Err(Error::ErrTooManyRequiredSigs));
    }

    #[test]
    fn test_empty_keys() {
        let result = multisig_redeem_script_sorted(empty::<Address>(), 0);
        assert_eq!(result, Err(Error::EmptyKeys));
    }

    #[test]
    fn test_wrong_version() {
        let payload = vec![0u8; 32];
        let address1 = Address::new(Prefix::Testnet, Version::PubKey, &payload);
        let address2 = Address::new(Prefix::Testnet, Version::ScriptHash, &payload);
        let pub_keys = vec![&address1, &address2];
        let result = multisig_redeem_script_sorted(pub_keys.into_iter(), 1);
        assert_eq!(result, Err(Error::WrongVersion));
    }

    fn check_multisig_scenario(mut inputs: Vec<Input>, required: usize, is_ok: bool, is_ecdsa: bool) {
        // Taken from: d839d29b549469d0f9a23e51febe68d4084967a6a477868b511a5a8d88c5ae06
        let prev_tx_id = TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap();
        inputs.sort_by_key(|v| v.kp.public_key());

        let addresses = inputs.iter().filter(|input| input.required).map(|input| {
            if !is_ecdsa {
                Address::new(Prefix::Testnet, Version::PubKey, &input.kp.x_only_public_key().0.serialize())
            } else {
                Address::new(Prefix::Testnet, Version::PubKeyECDSA, &input.kp.public_key().serialize())
            }
        });
        let mut addresses_second_iter = addresses.clone();
        let script = multisig_redeem_script_sorted(addresses, required).unwrap();
        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 4,
            }],
            vec![
                TransactionOutput {
                    value: 10000000000000,
                    script_public_key: pay_to_address_script(&addresses_second_iter.next().unwrap()),
                },
                TransactionOutput {
                    value: 2792999990000,
                    script_public_key: pay_to_address_script(&addresses_second_iter.next().unwrap()),
                },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let entries = vec![UtxoEntry {
            amount: 12793000000000,
            script_public_key: pay_to_script_hash_script(&script),
            block_daa_score: 36151168,
            is_coinbase: false,
        }];
        let mut tx = MutableTransaction::with_entries(tx, entries);

        let mut reused_values = SigHashReusedValues::new();
        let sig_hash = if !is_ecdsa {
            calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &mut reused_values)
        } else {
            calc_ecdsa_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &mut reused_values)
        };
        let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
        let signatures: Vec<_> = inputs
            .iter()
            .filter(|input| input.sign)
            .flat_map(|input| {
                if !is_ecdsa {
                    let sig = *input.kp.sign_schnorr(msg).as_ref();
                    iter::once(OpData65).chain(sig).chain([SIG_HASH_ALL.to_u8()])
                } else {
                    let sig = input.kp.secret_key().sign_ecdsa(msg).serialize_compact();
                    iter::once(OpData65).chain(sig).chain([SIG_HASH_ALL.to_u8()])
                }
            })
            .collect();

        {
            tx.tx.inputs[0].signature_script =
                signatures.into_iter().chain(ScriptBuilder::new().add_data(&script).unwrap().drain()).collect();
        }

        let tx = tx.as_verifiable();
        let (input, entry) = tx.populated_inputs().next().unwrap();

        let cache = Cache::new(10_000);
        let mut engine = TxScriptEngine::from_transaction_input(&tx, input, 0, entry, &mut reused_values, &cache).unwrap();
        assert_eq!(engine.execute().is_ok(), is_ok);
    }
    #[test]
    fn test_multisig_1_2() {
        let [kp1, kp2, ..] = kp();
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: false }, Input { kp: kp2, required: true, sign: true }],
            1,
            true,
            false,
        );
        let [kp1, kp2, ..] = kp();
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: true }, Input { kp: kp2, required: true, sign: false }],
            1,
            true,
            false,
        );

        // ecdsa
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: false }, Input { kp: kp2, required: true, sign: true }],
            1,
            true,
            true,
        );
        let [kp1, kp2, ..] = kp();
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: true }, Input { kp: kp2, required: true, sign: false }],
            1,
            true,
            true,
        );
    }

    #[test]
    fn test_multisig_2_2() {
        let [kp1, kp2, ..] = kp();
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: true }, Input { kp: kp2, required: true, sign: true }],
            2,
            true,
            false,
        );

        // ecdsa
        let [kp1, kp2, ..] = kp();
        check_multisig_scenario(
            vec![Input { kp: kp1, required: true, sign: true }, Input { kp: kp2, required: true, sign: true }],
            2,
            true,
            true,
        );
    }

    #[test]
    fn test_multisig_wrong_signer() {
        let [kp1, kp2, kp3] = kp();
        check_multisig_scenario(
            vec![
                Input { kp: kp1, required: true, sign: false },
                Input { kp: kp2, required: true, sign: false },
                Input { kp: kp3, required: false, sign: true },
            ],
            1,
            false,
            false,
        );

        // ecdsa
        let [kp1, kp2, kp3] = kp();
        check_multisig_scenario(
            vec![
                Input { kp: kp1, required: true, sign: false },
                Input { kp: kp2, required: true, sign: false },
                Input { kp: kp3, required: false, sign: true },
            ],
            1,
            false,
            true,
        );
    }

    #[test]
    fn test_multisig_not_enough() {
        let [kp1, kp2, kp3] = kp();
        check_multisig_scenario(
            vec![
                Input { kp: kp1, required: true, sign: true },
                Input { kp: kp2, required: true, sign: true },
                Input { kp: kp3, required: true, sign: false },
            ],
            3,
            false,
            false,
        );

        let [kp1, kp2, kp3] = kp();
        check_multisig_scenario(
            vec![
                Input { kp: kp1, required: true, sign: true },
                Input { kp: kp2, required: true, sign: true },
                Input { kp: kp3, required: true, sign: false },
            ],
            3,
            false,
            true,
        );
    }
}
