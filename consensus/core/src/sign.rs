use crate::{
    hashing::{
        sighash::{calc_schnorr_signature_hash, SigHashReusedValues},
        sighash_type::SIG_HASH_ALL,
    },
    tx::SignableTransaction,
};
use itertools::Itertools;
use std::collections::BTreeMap;
use std::iter::once;
use thiserror::Error;
//use workflow_log::log_trace;

#[derive(Error, Debug, Clone)]
pub enum Error {
    #[error("{0}")]
    Message(String),

    #[error("Secp256k1 -> {0}")]
    Secp256k1Error(#[from] secp256k1::Error),
}

// // Sign a transaction using schnorr
// fn _sign(mut signable_tx: SignableTransaction, privkey: [u8; 32]) -> SignableTransaction {
//     for i in 0..signable_tx.tx.inputs.len() {
//         signable_tx.tx.inputs[i].sig_op_count = 1;
//     }

//     let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, &privkey).unwrap();
//     let mut reused_values = SigHashReusedValues::new();
//     for i in 0..signable_tx.tx.inputs.len() {
//         let sig_hash = calc_schnorr_signature_hash(&signable_tx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values);
//         let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
//         let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
//         // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
//         signable_tx.tx.inputs[i].signature_script = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
//     }
//     signable_tx
// }

/// Sign a transaction using schnorr
pub fn sign(mut signable_tx: SignableTransaction, schnorr_key: secp256k1::KeyPair) -> SignableTransaction {
    for i in 0..signable_tx.tx.inputs.len() {
        signable_tx.tx.inputs[i].sig_op_count = 1;
    }

    let mut reused_values = SigHashReusedValues::new();
    for i in 0..signable_tx.tx.inputs.len() {
        let sig_hash = calc_schnorr_signature_hash(&signable_tx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values);
        let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
        let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
        // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
        signable_tx.tx.inputs[i].signature_script = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
    }
    signable_tx
}

/// Sign a transaction using schnorr
pub fn sign_with_multiple(mut mutable_tx: SignableTransaction, privkeys: Vec<[u8; 32]>) -> SignableTransaction {
    let mut map = BTreeMap::new();
    for privkey in privkeys {
        let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, &privkey).unwrap();
        map.insert(schnorr_key.public_key().serialize(), schnorr_key);
    }
    for i in 0..mutable_tx.tx.inputs.len() {
        mutable_tx.tx.inputs[i].sig_op_count = 1;
    }

    let mut reused_values = SigHashReusedValues::new();
    for i in 0..mutable_tx.tx.inputs.len() {
        let script = mutable_tx.entries[i].as_ref().unwrap().script_public_key.script();
        if let Some(schnorr_key) = map.get(script) {
            let sig_hash = calc_schnorr_signature_hash(&mutable_tx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values);
            let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
            let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
            // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
            mutable_tx.tx.inputs[i].signature_script = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
        }
    }
    mutable_tx
}

/// Sign a transaction using schnorr
pub fn sign_with_multiple_v2(mut mutable_tx: SignableTransaction, privkeys: Vec<[u8; 32]>) -> SignableTransaction {
    let mut map = BTreeMap::new();
    for privkey in privkeys {
        let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, &privkey).unwrap();
        let schnorr_public_key = schnorr_key.public_key().x_only_public_key().0;
        let script_pub_key_script = once(0x20).chain(schnorr_public_key.serialize().into_iter()).chain(once(0xac)).collect_vec();
        //workflow_log::log_info!("schnorr_public_key {script_pub_key_script:?}");
        map.insert(script_pub_key_script, schnorr_key);
    }

    let mut reused_values = SigHashReusedValues::new();
    for i in 0..mutable_tx.tx.inputs.len() {
        let script = mutable_tx.entries[i].as_ref().unwrap().script_public_key.script();
        //workflow_log::log_info!("script_public_key.script {script:?}");
        if let Some(schnorr_key) = map.get(script) {
            let sig_hash = calc_schnorr_signature_hash(&mutable_tx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values);
            let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
            let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
            // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
            //workflow_log::log_info!("signature_script {sig:?}");
            mutable_tx.tx.inputs[i].signature_script = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
        }
    }
    mutable_tx
}

pub fn verify(tx: &impl crate::tx::VerifiableTransaction) -> Result<(), Error> {
    let mut reused_values = SigHashReusedValues::new();
    for (i, (input, entry)) in tx.populated_inputs().enumerate() {
        //log_trace!("input({i}).signature_script.len(): {}", input.signature_script.len());
        if input.signature_script.is_empty() {
            return Err(Error::Message(format!("Signature is empty for input: {i}")));
            //return Err(secp256k1::Error::InvalidSignature.into());
        }
        let pk = &entry.script_public_key.script()[1..33];
        //log_trace!("pk: {pk:?}");
        let pk = secp256k1::XOnlyPublicKey::from_slice(pk)?;
        //log_trace!("xonly pk: {pk:?}");
        let sig = secp256k1::schnorr::Signature::from_slice(&input.signature_script[1..65])?;
        //log_trace!("sig: {sig:?}");
        let sig_hash = calc_schnorr_signature_hash(tx, i, SIG_HASH_ALL, &mut reused_values);
        //log_trace!("sig_hash: {sig_hash:?}");
        let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice())?;
        //log_trace!("msg: {msg:?}");
        sig.verify(&msg, &pk)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{subnets::SubnetworkId, tx::*};
    use secp256k1::{rand, Secp256k1};
    use std::str::FromStr;

    #[test]
    fn test_and_verify_sign() {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let script_pub_key = ScriptVec::from_slice(&public_key.serialize());

        let (secret_key2, public_key2) = secp.generate_keypair(&mut rand::thread_rng());
        let script_pub_key2 = ScriptVec::from_slice(&public_key2.serialize());

        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let unsigned_tx = Transaction::new(
            0,
            vec![
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                    signature_script: vec![],
                    sequence: 0,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                    signature_script: vec![],
                    sequence: 1,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 2 },
                    signature_script: vec![],
                    sequence: 2,
                    sig_op_count: 0,
                },
            ],
            vec![
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
            ],
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let entries = vec![
            UtxoEntry {
                amount: 100,
                script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
                block_daa_score: 0,
                is_coinbase: false,
            },
            UtxoEntry {
                amount: 200,
                script_public_key: ScriptPublicKey::new(0, script_pub_key),
                block_daa_score: 0,
                is_coinbase: false,
            },
            UtxoEntry {
                amount: 300,
                script_public_key: ScriptPublicKey::new(0, script_pub_key2),
                block_daa_score: 0,
                is_coinbase: false,
            },
        ];
        let signed_tx = sign_with_multiple(
            SignableTransaction::with_entries(unsigned_tx, entries),
            vec![secret_key.secret_bytes(), secret_key2.secret_bytes()],
        );
        // sign(SignableTransaction::with_entries(unsigned_tx, entries), vec![secret_key.secret_bytes(), secret_key2.secret_bytes()]);
        assert!(verify(&signed_tx.as_verifiable()).is_ok());
    }
}
