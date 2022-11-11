use hashes::{Hash, Hasher, HasherBase, TransactionSigningHash};

use crate::{
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{PopulatedTransaction, ScriptPublicKey, TransactionOutpoint, TransactionOutput},
};

use super::{sighash_type::SigHashType, HasherExtensions};

#[derive(Default)]
pub struct SigHashReusedValues {
    previous_outputs_hash: Option<Hash>,
    sequence_hash: Option<Hash>,
    sig_op_counts_hash: Option<Hash>,
    outputs_hash: Option<Hash>,
}

impl SigHashReusedValues {
    pub fn new() -> Self {
        Self { previous_outputs_hash: None, sequence_hash: None, sig_op_counts_hash: None, outputs_hash: None }
    }
}

fn previous_output_hash(tx: &PopulatedTransaction, hash_type: SigHashType, reused_values: &mut SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_anyone_can_pay() {
        return 0.into();
    }

    if let Some(previous_outputs_hash) = reused_values.previous_outputs_hash {
        previous_outputs_hash
    } else {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.tx.inputs.iter() {
            hasher.update(input.previous_outpoint.transaction_id.as_bytes());
            hasher.write_u32(input.previous_outpoint.index);
        }
        let previous_outputs_hash = hasher.finalize();
        reused_values.previous_outputs_hash = Some(previous_outputs_hash);
        previous_outputs_hash
    }
}

fn sequence_hash(tx: &PopulatedTransaction, hash_type: SigHashType, reused_values: &mut SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_single() || hash_type.is_sighash_anyone_can_pay() || hash_type.is_sighash_none() {
        return 0.into();
    }

    if let Some(sequence_hash) = reused_values.sequence_hash {
        sequence_hash
    } else {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.tx.inputs.iter() {
            hasher.write_u64(input.sequence);
        }
        let sequence_hash = hasher.finalize();
        reused_values.sequence_hash = Some(sequence_hash);
        sequence_hash
    }
}

fn sig_op_counts_hash(tx: &PopulatedTransaction, hash_type: SigHashType, reused_values: &mut SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_anyone_can_pay() {
        return 0.into();
    }

    if let Some(sig_op_counts_hash) = reused_values.sig_op_counts_hash {
        sig_op_counts_hash
    } else {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.tx.inputs.iter() {
            hasher.write_u8(input.sig_op_count);
        }
        let sig_op_counts_hash = hasher.finalize();
        reused_values.sig_op_counts_hash = Some(sig_op_counts_hash);
        sig_op_counts_hash
    }
}

fn payload_hash(tx: &PopulatedTransaction) -> Hash {
    if tx.tx.subnetwork_id == SUBNETWORK_ID_NATIVE {
        return 0.into();
    }

    // TODO: Right now this branch will never be executed, since payload is disabled
    // for all non coinbase transactions. Once payload is enabled, the payload hash
    // should be cached to make it cost O(1) instead of O(tx.inputs.len()).
    let mut hasher = TransactionSigningHash::new();
    hasher.write_var_bytes(&tx.tx.payload);
    hasher.finalize()
}

fn outputs_hash(
    tx: &PopulatedTransaction,
    hash_type: SigHashType,
    reused_values: &mut SigHashReusedValues,
    input_index: usize,
) -> Hash {
    if hash_type.is_sighash_none() {
        return 0.into();
    }

    if hash_type.is_sighash_single() {
        // If the relevant output exists - return its hash, otherwise return zero-hash
        if input_index >= tx.outputs().len() {
            return 0.into();
        }

        let mut hasher = TransactionSigningHash::new();
        hash_output(&mut hasher, &tx.outputs()[input_index]);
        return hasher.finalize();
    }

    // Otherwise, return hash of all outputs. Re-use hash if available.
    if let Some(outputs_hash) = reused_values.outputs_hash {
        outputs_hash
    } else {
        let mut hasher = TransactionSigningHash::new();
        for output in tx.tx.outputs.iter() {
            hash_output(&mut hasher, output);
        }
        let outputs_hash = hasher.finalize();
        reused_values.outputs_hash = Some(outputs_hash);
        outputs_hash
    }
}

fn hash_outpoint(hasher: &mut impl Hasher, outpoint: TransactionOutpoint) {
    hasher.update(outpoint.transaction_id);
    hasher.write_u32(outpoint.index);
}

fn hash_output(hasher: &mut impl Hasher, output: &TransactionOutput) {
    hasher.write_u64(output.value);
    hash_script_public_key(hasher, &output.script_public_key);
}

fn hash_script_public_key(hasher: &mut impl Hasher, script_public_key: &ScriptPublicKey) {
    hasher.write_u16(script_public_key.version());
    hasher.write_var_bytes(script_public_key.script());
}

pub fn calc_schnorr_signature_hash(
    tx: &PopulatedTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &mut SigHashReusedValues,
) -> Hash {
    let input = tx.populated_inputs().nth(input_index).unwrap();
    let mut hasher = TransactionSigningHash::new();
    hasher.write_u16(tx.tx.version);
    hasher.update(previous_output_hash(tx, hash_type, reused_values));
    hasher.update(sequence_hash(tx, hash_type, reused_values));
    hasher.update(sig_op_counts_hash(tx, hash_type, reused_values));
    hash_outpoint(&mut hasher, input.0.previous_outpoint);
    hash_script_public_key(&mut hasher, &input.1.script_public_key);
    hasher.write_u64(input.1.amount);
    hasher.write_u64(input.0.sequence);
    hasher.write_u8(input.0.sig_op_count);
    hasher.update(outputs_hash(tx, hash_type, reused_values, input_index));
    hasher.write_u64(tx.tx.lock_time);
    hasher.update(&tx.tx.subnetwork_id);
    hasher.write_u64(tx.tx.gas);
    hasher.update(payload_hash(tx));
    hasher.write_u8(hash_type.to_u8());
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use smallvec::SmallVec;

    use crate::{
        hashing::sighash_type::SIG_HASH_ALL,
        subnets::SubnetworkId,
        tx::{Transaction, TransactionId, TransactionInput, UtxoEntry},
    };

    use super::*;

    #[test]
    fn test_signature_hash() {
        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("208325613d2eeaf7176ac6c670b13c0043156c427438ed72d74b7800862ad884e8ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("20fcef4c106cf11135bbd70f02a726a92162d2fb8b22f0469126f800862ad884e8ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_2 = SmallVec::from_vec(bytes.to_vec());

        let tx = Transaction::new(
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
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()) },
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) },
            ],
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![
                UtxoEntry {
                    amount: 100,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_1),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
                UtxoEntry {
                    amount: 200,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
                UtxoEntry {
                    amount: 300,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
            ],
        );

        let mut reused_values = SigHashReusedValues::new();
        assert_eq!(
            calc_schnorr_signature_hash(&populated_tx, 0, SIG_HASH_ALL, &mut reused_values).to_string(),
            "b363613fe99c8bb1d3712656ec8dfaea621ee6a9a95d851aec5bb59363b03f5e"
        );
    }
}
