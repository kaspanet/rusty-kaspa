use crate::opcodes::codes::{OpCheckMultiSig, OpCheckMultiSigECDSA};
use crate::script_builder::{ScriptBuilder, ScriptBuilderError};
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
    #[error("provided public keys should not be empty")]
    EmptyKeys,
}
pub fn multisig_redeem_script(pub_keys: impl Iterator<Item = impl Borrow<[u8; 32]>>, required: usize) -> Result<Vec<u8>, Error> {
    if pub_keys.size_hint().1.is_some_and(|upper| upper < required) {
        return Err(Error::ErrTooManyRequiredSigs);
    }
    let mut builder = ScriptBuilder::new();
    builder.add_i64(required as i64)?;

    let mut count = 0i64;
    for pub_key in pub_keys {
        count += 1;
        builder.add_data(pub_key.borrow().as_slice())?;
    }

    if (count as usize) < required {
        return Err(Error::ErrTooManyRequiredSigs);
    }
    if count == 0 {
        return Err(Error::EmptyKeys);
    }

    builder.add_i64(count)?;
    builder.add_op(OpCheckMultiSig)?;

    Ok(builder.drain())
}

pub fn multisig_redeem_script_ecdsa(pub_keys: impl Iterator<Item = impl Borrow<[u8; 33]>>, required: usize) -> Result<Vec<u8>, Error> {
    if pub_keys.size_hint().1.is_some_and(|upper| upper < required) {
        return Err(Error::ErrTooManyRequiredSigs);
    }
    let mut builder = ScriptBuilder::new();
    builder.add_i64(required as i64)?;

    let mut count = 0i64;
    for pub_key in pub_keys {
        count += 1;
        builder.add_data(pub_key.borrow().as_slice())?;
    }

    if (count as usize) < required {
        return Err(Error::ErrTooManyRequiredSigs);
    }
    if count == 0 {
        return Err(Error::EmptyKeys);
    }

    builder.add_i64(count)?;
    builder.add_op(OpCheckMultiSigECDSA)?;

    Ok(builder.drain())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{caches::Cache, opcodes::codes::OpData65, pay_to_script_hash_script, TxScriptEngine};
    use core::str::FromStr;
    use kaspa_consensus_core::{
        hashing::{
            sighash::{calc_ecdsa_signature_hash, calc_schnorr_signature_hash, SigHashReusedValues},
            sighash_type::SIG_HASH_ALL,
        },
        subnets::SubnetworkId,
        tx::*,
    };
    use rand::thread_rng;
    use secp256k1::Keypair;
    use std::{iter, iter::empty};

    struct Input {
        kp: Keypair,
        required: bool,
        sign: bool,
    }

    fn kp() -> [Keypair; 3] {
        let kp1 = Keypair::from_seckey_slice(
            secp256k1::SECP256K1,
            hex::decode("1d99c236b1f37b3b845336e6c568ba37e9ced4769d83b7a096eec446b940d160").unwrap().as_slice(),
        )
        .unwrap();
        let kp2 = Keypair::from_seckey_slice(
            secp256k1::SECP256K1,
            hex::decode("349ca0c824948fed8c2c568ce205e9d9be4468ef099cad76e3e5ec918954aca4").unwrap().as_slice(),
        )
        .unwrap();
        let kp3 = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());
        [kp1, kp2, kp3]
    }

    #[test]
    fn test_too_many_required_sigs() {
        let result = multisig_redeem_script(iter::once([0u8; 32]), 2);
        assert_eq!(result, Err(Error::ErrTooManyRequiredSigs));
        let result = multisig_redeem_script_ecdsa(iter::once(&[0u8; 33]), 2);
        assert_eq!(result, Err(Error::ErrTooManyRequiredSigs));
    }

    #[test]
    fn test_empty_keys() {
        let result = multisig_redeem_script(empty::<[u8; 32]>(), 0);
        assert_eq!(result, Err(Error::EmptyKeys));
    }

    fn check_multisig_scenario(inputs: Vec<Input>, required: usize, is_ok: bool, is_ecdsa: bool) {
        // Taken from: d839d29b549469d0f9a23e51febe68d4084967a6a477868b511a5a8d88c5ae06
        let prev_tx_id = TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap();
        let filtered = inputs.iter().filter(|input| input.required);
        let script = if !is_ecdsa {
            let pks = filtered.map(|input| input.kp.x_only_public_key().0.serialize());
            multisig_redeem_script(pks, required).unwrap()
        } else {
            let pks = filtered.map(|input| input.kp.public_key().serialize());
            multisig_redeem_script_ecdsa(pks, required).unwrap()
        };

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 4,
            }],
            vec![],
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
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();
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
