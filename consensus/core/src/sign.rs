use crate::{
    hashing::{
        sighash::{calc_schnorr_signature_hash, SigHashReusedValues},
        sighash_type::SIG_HASH_ALL,
    },
    tx::MutableTransaction,
};

/// Sign a transaction using schnorr
pub fn sign(mut mutable_tx: MutableTransaction, privkey: [u8; 32]) -> MutableTransaction {
    let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, &privkey).unwrap();
    let mut reused_values = SigHashReusedValues::new();
    for i in 0..mutable_tx.tx.inputs.len() {
        let sig_hash = calc_schnorr_signature_hash(&mutable_tx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values);
        let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
        let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
        // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
        mutable_tx.tx.inputs[i].signature_script = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
        // TODO: update sig_op_counts
    }
    mutable_tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{subnets::SubnetworkId, tx::*};
    use secp256k1::{rand, Secp256k1};
    use std::str::FromStr;

    fn verify(tx: &impl VerifiableTransaction) -> Result<(), secp256k1::Error> {
        let mut reused_values = SigHashReusedValues::new();
        for (i, (input, entry)) in tx.populated_inputs().enumerate() {
            let pk = &entry.script_public_key.script()[1..33];
            let pk = secp256k1::XOnlyPublicKey::from_slice(pk).unwrap();
            let sig = secp256k1::schnorr::Signature::from_slice(&input.signature_script[1..65]).unwrap();
            let sig_hash = calc_schnorr_signature_hash(tx, i, SIG_HASH_ALL, &mut reused_values);
            let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
            sig.verify(&msg, &pk)?;
        }

        Ok(())
    }

    #[test]
    fn test_sign() {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let script_pub_key = ScriptVec::from_slice(&public_key.serialize());

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
                script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
                block_daa_score: 0,
                is_coinbase: false,
            },
            UtxoEntry {
                amount: 300,
                script_public_key: ScriptPublicKey::new(0, script_pub_key),
                block_daa_score: 0,
                is_coinbase: false,
            },
        ];
        let signed_tx = sign(MutableTransaction::with_entries(unsigned_tx, entries), secret_key.secret_bytes());
        assert!(verify(&signed_tx.as_verifiable()).is_ok());
    }
}
