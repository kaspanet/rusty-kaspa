use crate::opcodes::codes::{OpCheckLockTimeVerify, OpCheckSig, OpDup, OpElse, OpEndIf, OpEqualVerify, OpIf, OpSHA256};
use crate::script_builder::{ScriptBuilder, ScriptBuilderResult};

pub fn htlc_redeem_script(receiver_pubkey: &[u8], sender_pubkey: &[u8], hash: &[u8], locktime: u64) -> ScriptBuilderResult<Vec<u8>> {
    let mut builder = ScriptBuilder::new();
    builder
        // withdraw branch
        .add_op(OpIf)?
        .add_op(OpSHA256)?
        .add_data(hash)?
        .add_op(OpEqualVerify)?
        .add_op(OpDup)?
        .add_data(receiver_pubkey)?
        .add_op(OpEqualVerify)?
        .add_op(OpCheckSig)?
        // refund branch
        .add_op(OpElse)?
        .add_lock_time(locktime)?
        .add_op(OpCheckLockTimeVerify)?
        .add_op(OpDup)?
        .add_data(sender_pubkey)?
        .add_op(OpEqualVerify)?
        .add_op(OpCheckSig)?
        // end
        .add_op(OpEndIf)?;

    Ok(builder.drain())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::Cache;
    use crate::opcodes::codes::{OpFalse, OpTrue};
    use crate::{pay_to_script_hash_script, TxScriptEngine};
    use kaspa_consensus_core::{
        hashing::{
            sighash::{calc_schnorr_signature_hash, SigHashReusedValues},
            sighash_type::SIG_HASH_ALL,
        },
        subnets::SubnetworkId,
        tx::{
            MutableTransaction, Transaction, TransactionId, TransactionInput, TransactionOutpoint, UtxoEntry, VerifiableTransaction,
        },
    };
    use rand::thread_rng;
    use secp256k1::KeyPair;
    use sha2::{Digest, Sha256};
    use std::str::FromStr;

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
    fn test_htlc() {
        let [receiver, sender, ..] = kp();

        let mut hasher = Sha256::new();
        hasher.update(b"hello world");
        let result = hasher.finalize();
        let hash = &result[..];

        let script = htlc_redeem_script(
            receiver.x_only_public_key().0.serialize().as_slice(),
            sender.x_only_public_key().0.serialize().as_slice(),
            hash,
            100,
        )
        .unwrap();

        // Taken from: d839d29b549469d0f9a23e51febe68d4084967a6a477868b511a5a8d88c5ae06
        let prev_tx_id = TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap();

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: vec![],
                sequence: 101,
                sig_op_count: 4,
            }],
            vec![],
            101,
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

        // check witdraw
        {
            let mut tx = MutableTransaction::with_entries(tx.clone(), entries.clone());
            let mut reused_values = SigHashReusedValues::new();
            let sig_hash = calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &mut reused_values);
            let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();

            let sig = receiver.sign_schnorr(msg);
            let mut signature = Vec::new();
            signature.extend_from_slice(sig.as_ref().as_slice());
            signature.push(SIG_HASH_ALL.to_u8());

            let mut builder = ScriptBuilder::new();
            builder.add_data(&signature).unwrap();
            builder.add_data(receiver.x_only_public_key().0.serialize().as_slice()).unwrap();
            builder.add_data(b"hello world").unwrap();
            builder.add_op(OpTrue).unwrap();
            builder.add_data(&script).unwrap();
            {
                tx.tx.inputs[0].signature_script = builder.drain();
            }

            let tx = tx.as_verifiable();
            let (input, entry) = tx.populated_inputs().next().unwrap();

            let cache = Cache::new(10_000);
            let mut engine = TxScriptEngine::from_transaction_input(&tx, input, 0, entry, &mut reused_values, &cache).unwrap();
            assert_eq!(engine.execute().is_ok(), true);
        }

        // check refund
        {
            let mut tx = MutableTransaction::with_entries(tx, entries);
            let mut reused_values = SigHashReusedValues::new();
            let sig_hash = calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &mut reused_values);
            let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();

            let sig = sender.sign_schnorr(msg);
            let mut signature = Vec::new();
            signature.extend_from_slice(sig.as_ref().as_slice());
            signature.push(SIG_HASH_ALL.to_u8());

            let mut builder = ScriptBuilder::new();
            builder.add_data(&signature).unwrap();
            builder.add_data(sender.x_only_public_key().0.serialize().as_slice()).unwrap();
            builder.add_op(OpFalse).unwrap();
            builder.add_data(&script).unwrap();
            {
                tx.tx.inputs[0].signature_script = builder.drain();
            }

            let tx = tx.as_verifiable();
            let (input, entry) = tx.populated_inputs().next().unwrap();

            let cache = Cache::new(10_000);
            let mut engine = TxScriptEngine::from_transaction_input(&tx, input, 0, entry, &mut reused_values, &cache).unwrap();
            assert_eq!(engine.execute().is_ok(), true);
        }
    }
}
