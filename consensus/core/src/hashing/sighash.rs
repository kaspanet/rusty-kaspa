use arc_swap::ArcSwapOption;
use kaspa_hashes::{Hash, Hasher, HasherBase, TransactionSigningHash, TransactionSigningHashECDSA, ZERO_HASH};
use std::cell::Cell;
use std::sync::Arc;

use crate::{
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{ScriptPublicKey, Transaction, TransactionOutpoint, TransactionOutput, VerifiableTransaction},
};

use super::{sighash_type::SigHashType, HasherExtensions};

/// Holds all fields used in the calculation of a transaction's sig_hash which are
/// the same for all transaction inputs.
/// Reuse of such values prevents the quadratic hashing problem.
#[derive(Default)]
pub struct SigHashReusedValuesUnsync {
    previous_outputs_hash: Cell<Option<Hash>>,
    sequences_hash: Cell<Option<Hash>>,
    sig_op_counts_hash: Cell<Option<Hash>>,
    outputs_hash: Cell<Option<Hash>>,
}

impl SigHashReusedValuesUnsync {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Default)]
pub struct SigHashReusedValuesSync {
    previous_outputs_hash: ArcSwapOption<Hash>,
    sequences_hash: ArcSwapOption<Hash>,
    sig_op_counts_hash: ArcSwapOption<Hash>,
    outputs_hash: ArcSwapOption<Hash>,
}

impl SigHashReusedValuesSync {
    pub fn new() -> Self {
        Self::default()
    }
}

pub trait SigHashReusedValues {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash) -> Hash;
    fn sequences_hash(&self, set: impl Fn() -> Hash) -> Hash;

    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash) -> Hash;

    fn outputs_hash(&self, set: impl Fn() -> Hash) -> Hash;
}

impl<T: SigHashReusedValues> SigHashReusedValues for Arc<T> {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.as_ref().previous_outputs_hash(set)
    }

    fn sequences_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.as_ref().sequences_hash(set)
    }

    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.as_ref().sig_op_counts_hash(set)
    }

    fn outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.as_ref().outputs_hash(set)
    }
}

impl SigHashReusedValues for SigHashReusedValuesUnsync {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.previous_outputs_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.previous_outputs_hash.set(Some(hash));
            hash
        })
    }

    fn sequences_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.sequences_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.sequences_hash.set(Some(hash));
            hash
        })
    }

    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.sig_op_counts_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.sig_op_counts_hash.set(Some(hash));
            hash
        })
    }

    fn outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.outputs_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.outputs_hash.set(Some(hash));
            hash
        })
    }
}

impl SigHashReusedValues for SigHashReusedValuesSync {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.previous_outputs_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.previous_outputs_hash.rcu(|_| Arc::new(hash));
        hash
    }

    fn sequences_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.sequences_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.sequences_hash.rcu(|_| Arc::new(hash));
        hash
    }

    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.sig_op_counts_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.sig_op_counts_hash.rcu(|_| Arc::new(hash));
        hash
    }

    fn outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.outputs_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.outputs_hash.rcu(|_| Arc::new(hash));
        hash
    }
}

pub fn previous_outputs_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_anyone_can_pay() {
        return ZERO_HASH;
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.inputs.iter() {
            hasher.update(input.previous_outpoint.transaction_id.as_bytes());
            hasher.write_u32(input.previous_outpoint.index);
        }
        hasher.finalize()
    };
    reused_values.previous_outputs_hash(hash)
}

pub fn sequences_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_single() || hash_type.is_sighash_anyone_can_pay() || hash_type.is_sighash_none() {
        return ZERO_HASH;
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.inputs.iter() {
            hasher.write_u64(input.sequence);
        }
        hasher.finalize()
    };
    reused_values.sequences_hash(hash)
}

pub fn sig_op_counts_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_anyone_can_pay() {
        return ZERO_HASH;
    }

    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.inputs.iter() {
            hasher.write_u8(input.sig_op_count);
        }
        hasher.finalize()
    };
    reused_values.sig_op_counts_hash(hash)
}

pub fn payload_hash(tx: &Transaction) -> Hash {
    if tx.subnetwork_id == SUBNETWORK_ID_NATIVE {
        return ZERO_HASH;
    }

    // TODO: Right now this branch will never be executed, since payload is disabled
    // for all non coinbase transactions. Once payload is enabled, the payload hash
    // should be cached to make it cost O(1) instead of O(tx.inputs.len()).
    let mut hasher = TransactionSigningHash::new();
    hasher.write_var_bytes(&tx.payload);
    hasher.finalize()
}

pub fn outputs_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues, input_index: usize) -> Hash {
    if hash_type.is_sighash_none() {
        return ZERO_HASH;
    }

    if hash_type.is_sighash_single() {
        // If the relevant output exists - return its hash, otherwise return zero-hash
        if input_index >= tx.outputs.len() {
            return ZERO_HASH;
        }

        let mut hasher = TransactionSigningHash::new();
        hash_output(&mut hasher, &tx.outputs[input_index]);
        return hasher.finalize();
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for output in tx.outputs.iter() {
            hash_output(&mut hasher, output);
        }
        hasher.finalize()
    };
    // Otherwise, return hash of all outputs. Re-use hash if available.
    reused_values.outputs_hash(hash)
}

pub fn hash_outpoint(hasher: &mut impl Hasher, outpoint: TransactionOutpoint) {
    hasher.update(outpoint.transaction_id);
    hasher.write_u32(outpoint.index);
}

pub fn hash_output(hasher: &mut impl Hasher, output: &TransactionOutput) {
    hasher.write_u64(output.value);
    hash_script_public_key(hasher, &output.script_public_key);
}

pub fn hash_script_public_key(hasher: &mut impl Hasher, script_public_key: &ScriptPublicKey) {
    hasher.write_u16(script_public_key.version());
    hasher.write_var_bytes(script_public_key.script());
}

pub fn calc_schnorr_signature_hash(
    verifiable_tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &impl SigHashReusedValues,
) -> Hash {
    let input = verifiable_tx.populated_input(input_index);
    let tx = verifiable_tx.tx();
    let mut hasher = TransactionSigningHash::new();
    hasher
        .write_u16(tx.version)
        .update(previous_outputs_hash(tx, hash_type, reused_values))
        .update(sequences_hash(tx, hash_type, reused_values))
        .update(sig_op_counts_hash(tx, hash_type, reused_values));
    hash_outpoint(&mut hasher, input.0.previous_outpoint);
    hash_script_public_key(&mut hasher, &input.1.script_public_key);
    hasher
        .write_u64(input.1.amount)
        .write_u64(input.0.sequence)
        .write_u8(input.0.sig_op_count)
        .update(outputs_hash(tx, hash_type, reused_values, input_index))
        .write_u64(tx.lock_time)
        .update(&tx.subnetwork_id)
        .write_u64(tx.gas)
        .update(payload_hash(tx))
        .write_u8(hash_type.to_u8());
    hasher.finalize()
}

pub fn calc_ecdsa_signature_hash(
    tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &impl SigHashReusedValues,
) -> Hash {
    let hash = calc_schnorr_signature_hash(tx, input_index, hash_type, reused_values);
    let mut hasher = TransactionSigningHashECDSA::new();
    hasher.update(hash);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, vec};

    use smallvec::SmallVec;

    use crate::{
        hashing::sighash_type::{SIG_HASH_ALL, SIG_HASH_ANY_ONE_CAN_PAY, SIG_HASH_NONE, SIG_HASH_SINGLE},
        subnets::SubnetworkId,
        tx::{PopulatedTransaction, Transaction, TransactionId, TransactionInput, UtxoEntry},
    };

    use super::*;

    #[test]
    fn test_signature_hash() {
        // TODO: Copy all sighash tests from go kaspad.
        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("208325613d2eeaf7176ac6c670b13c0043156c427438ed72d74b7800862ad884e8ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("20fcef4c106cf11135bbd70f02a726a92162d2fb8b22f0469126f800862ad884e8ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_2 = SmallVec::from_vec(bytes.to_vec());

        let native_tx = Transaction::new(
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
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );

        let native_populated_tx = PopulatedTransaction::new(
            &native_tx,
            vec![
                UtxoEntry {
                    amount: 100,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()),
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
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
            ],
        );

        let mut subnetwork_tx = native_tx.clone();
        subnetwork_tx.subnetwork_id = SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        subnetwork_tx.gas = 250;
        subnetwork_tx.payload = vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
        let subnetwork_populated_tx = PopulatedTransaction::new(
            &subnetwork_tx,
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

        enum ModifyAction {
            NoAction,
            Output(usize),
            Input(usize),
            AmountSpent(usize),
            PrevScriptPublicKey(usize),
            Sequence(usize),
            Payload,
            Gas,
            SubnetworkId,
        }

        struct TestVector<'a> {
            name: &'static str,
            populated_tx: &'a PopulatedTransaction<'a>,
            hash_type: SigHashType,
            input_index: usize,
            action: ModifyAction,
            expected_hash: &'static str,
        }

        const SIG_HASH_ALL_ANYONE_CAN_PAY: SigHashType = SigHashType(SIG_HASH_ALL.0 | SIG_HASH_ANY_ONE_CAN_PAY.0);
        const SIG_HASH_NONE_ANYONE_CAN_PAY: SigHashType = SigHashType(SIG_HASH_NONE.0 | SIG_HASH_ANY_ONE_CAN_PAY.0);
        const SIG_HASH_SINGLE_ANYONE_CAN_PAY: SigHashType = SigHashType(SIG_HASH_SINGLE.0 | SIG_HASH_ANY_ONE_CAN_PAY.0);

        let tests = [
            // SIG_HASH_ALL
            TestVector {
                name: "native-all-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "03b7ac6927b2b67100734c3cc313ff8c2e8b3ce3e746d46dd660b706a916b1f5",
            },
            TestVector {
                name: "native-all-0-modify-input-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Input(1),
                expected_hash: "a9f563d86c0ef19ec2e4f483901d202e90150580b6123c3d492e26e7965f488c", // should change the hash
            },
            TestVector {
                name: "native-all-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Output(1),
                expected_hash: "aad2b61bd2405dfcf7294fc2be85f325694f02dda22d0af30381cb50d8295e0a", // should change the hash
            },
            TestVector {
                name: "native-all-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "0818bd0a3703638d4f01014c92cf866a8903cab36df2fa2506dc0d06b94295e8", // should change the hash
            },
            TestVector {
                name: "native-all-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "24821e466e53ff8e5fa93257cb17bb06131a48be4ef282e87f59d2bdc9afebc2", // should change the hash
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-input-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Input(0),
                expected_hash: "d09cb639f335ee69ac71f2ad43fd9e59052d38a7d0638de4cf989346588a7c38", // should change the hash
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-input-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Input(1),
                expected_hash: "24821e466e53ff8e5fa93257cb17bb06131a48be4ef282e87f59d2bdc9afebc2", // shouldn't change the hash
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-sequence",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "24821e466e53ff8e5fa93257cb17bb06131a48be4ef282e87f59d2bdc9afebc2", // shouldn't change the hash
            },
            // SIG_HASH_NONE
            TestVector {
                name: "native-none-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "38ce4bc93cf9116d2e377b33ff8449c665b7b5e2f2e65303c543b9afdaa4bbba", // should change the hash
            },
            TestVector {
                name: "native-none-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Output(1),
                expected_hash: "38ce4bc93cf9116d2e377b33ff8449c665b7b5e2f2e65303c543b9afdaa4bbba", // shouldn't change the hash
            },
            TestVector {
                name: "native-none-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Output(1),
                expected_hash: "38ce4bc93cf9116d2e377b33ff8449c665b7b5e2f2e65303c543b9afdaa4bbba", // should change the hash
            },
            TestVector {
                name: "native-none-0-modify-sequence-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Sequence(0),
                expected_hash: "d9efdd5edaa0d3fd0133ee3ab731d8c20e0a1b9f3c0581601ae2075db1109268", // shouldn't change the hash
            },
            TestVector {
                name: "native-none-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "38ce4bc93cf9116d2e377b33ff8449c665b7b5e2f2e65303c543b9afdaa4bbba", // should change the hash
            },
            TestVector {
                name: "native-none-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "06aa9f4239491e07bb2b6bda6b0657b921aeae51e193d2c5bf9e81439cfeafa0", // should change the hash
            },
            TestVector {
                name: "native-none-anyonecanpay-0-modify-amount-spent",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::AmountSpent(0),
                expected_hash: "f07f45f3634d3ea8c0f2cb676f56e20993edf9be07a83bf0dfdb3debcf1441bf", // should change the hash
            },
            TestVector {
                name: "native-none-anyonecanpay-0-modify-script-public-key",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::PrevScriptPublicKey(0),
                expected_hash: "20a525c54dc33b2a61201f05233c086dbe8e06e9515775181ed96550b4f2d714", // should change the hash
            },
            // SIG_HASH_SINGLE
            TestVector {
                name: "native-single-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "44a0b407ff7b239d447743dd503f7ad23db5b2ee4d25279bd3dffaf6b474e005", // should change the hash
            },
            TestVector {
                name: "native-single-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Output(1),
                expected_hash: "44a0b407ff7b239d447743dd503f7ad23db5b2ee4d25279bd3dffaf6b474e005", // should change the hash
            },
            TestVector {
                name: "native-single-0-modify-sequence-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Sequence(0),
                expected_hash: "83796d22879718eee1165d4aace667bb6778075dab579c32c57be945f466a451", // should change the hash
            },
            TestVector {
                name: "native-single-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "44a0b407ff7b239d447743dd503f7ad23db5b2ee4d25279bd3dffaf6b474e005", // shouldn't change the hash
            },
            TestVector {
                name: "native-single-2-no-corresponding-output",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 2,
                action: ModifyAction::NoAction,
                expected_hash: "022ad967192f39d8d5895d243e025ec14cc7a79708c5e364894d4eff3cecb1b0", // should change the hash
            },
            TestVector {
                name: "native-single-2-no-corresponding-output-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 2,
                action: ModifyAction::Output(1),
                expected_hash: "022ad967192f39d8d5895d243e025ec14cc7a79708c5e364894d4eff3cecb1b0", // shouldn't change the hash
            },
            TestVector {
                name: "native-single-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "43b20aba775050cf9ba8d5e48fc7ed2dc6c071d23f30382aea58b7c59cfb8ed7", // should change the hash
            },
            TestVector {
                name: "native-single-anyonecanpay-2-no-corresponding-output",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE_ANYONE_CAN_PAY,
                input_index: 2,
                action: ModifyAction::NoAction,
                expected_hash: "846689131fb08b77f83af1d3901076732ef09d3f8fdff945be89aa4300562e5f", // should change the hash
            },
            // subnetwork transaction
            TestVector {
                name: "subnetwork-all-0",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "b2f421c933eb7e1a91f1d9e1efa3f120fe419326c0dbac487752189522550e0c", // should change the hash
            },
            TestVector {
                name: "subnetwork-all-modify-payload",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Payload,
                expected_hash: "12ab63b9aea3d58db339245a9b6e9cb6075b2253615ce0fb18104d28de4435a1", // should change the hash
            },
            TestVector {
                name: "subnetwork-all-modify-gas",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Gas,
                expected_hash: "2501edfc0068d591160c4bd98646c6e6892cdc051182a8be3ccd6d67f104fd17", // should change the hash
            },
            TestVector {
                name: "subnetwork-all-subnetwork-id",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::SubnetworkId,
                expected_hash: "a5d1230ede0dfcfd522e04123a7bcd721462fed1d3a87352031a4f6e3c4389b6", // should change the hash
            },
        ];

        for test in tests {
            let mut tx = test.populated_tx.tx.clone();
            let mut entries = test.populated_tx.entries.clone();
            match test.action {
                ModifyAction::NoAction => {}
                ModifyAction::Output(i) => {
                    tx.outputs[i].value = 100;
                }
                ModifyAction::Input(i) => {
                    tx.inputs[i].previous_outpoint.index = 2;
                }
                ModifyAction::AmountSpent(i) => {
                    entries[i].amount = 666;
                }
                ModifyAction::PrevScriptPublicKey(i) => {
                    let mut script_vec = entries[i].script_public_key.script().to_vec();
                    script_vec.append(&mut vec![1, 2, 3]);
                    entries[i].script_public_key = ScriptPublicKey::new(entries[i].script_public_key.version(), script_vec.into());
                }
                ModifyAction::Sequence(i) => {
                    tx.inputs[i].sequence = 12345;
                }
                ModifyAction::Payload => tx.payload = vec![6, 6, 6, 4, 2, 0, 1, 3, 3, 7],
                ModifyAction::Gas => tx.gas = 1234,
                ModifyAction::SubnetworkId => {
                    tx.subnetwork_id = SubnetworkId::from_bytes([6, 6, 6, 4, 2, 0, 1, 3, 3, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
                }
            }
            let populated_tx = PopulatedTransaction::new(&tx, entries);
            let reused_values = SigHashReusedValuesUnsync::new();
            assert_eq!(
                calc_schnorr_signature_hash(&populated_tx, test.input_index, test.hash_type, &reused_values).to_string(),
                test.expected_hash,
                "test {} failed",
                test.name
            );
        }
    }
}
