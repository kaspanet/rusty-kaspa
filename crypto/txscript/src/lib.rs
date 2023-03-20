extern crate alloc;
extern crate core;

pub mod caches;
mod data_stack;
pub mod opcodes;
pub mod script_builder;
pub mod script_class;
pub mod standard;

use crate::caches::Cache;
use crate::data_stack::{DataStack, Stack};
use crate::opcodes::{deserialize_opcode_data, OpCodeImplementation};
use consensus_core::hashing::sighash::{calc_ecdsa_signature_hash, calc_schnorr_signature_hash, SigHashReusedValues};
use consensus_core::hashing::sighash_type::SigHashType;
use consensus_core::tx::{ScriptPublicKey, TransactionInput, UtxoEntry, VerifiableTransaction};
use itertools::Itertools;
use log::trace;
use opcodes::{codes, to_small_int, OpCond};
use script_class::ScriptClass;
use txscript_errors::TxScriptError;

pub mod prelude {
    pub use super::standard::*;
}
pub use standard::*;

pub const MAX_SCRIPT_PUBLIC_KEY_VERSION: u16 = 0;
pub const MAX_STACK_SIZE: usize = 244;
pub const MAX_SCRIPTS_SIZE: usize = 10_000;
pub const MAX_SCRIPT_ELEMENT_SIZE: usize = 520;
pub const MAX_OPS_PER_SCRIPT: i32 = 201;
pub const MAX_TX_IN_SEQUENCE_NUM: u64 = u64::MAX;
pub const SEQUENCE_LOCK_TIME_DISABLED: u64 = 1 << 63;
pub const SEQUENCE_LOCK_TIME_MASK: u64 = 0x00000000ffffffff;
pub const LOCK_TIME_THRESHOLD: u64 = 500_000_000_000;
pub const MAX_PUB_KEYS_PER_MUTLTISIG: i32 = 20;

// The last opcode that does not count toward operations.
// Note that this includes OP_RESERVED which counts as a push operation.
pub const NO_COST_OPCODE: u8 = 0x60;

#[derive(Clone, Hash, PartialEq, Eq)]
enum Signature {
    Secp256k1(secp256k1::schnorr::Signature),
    Ecdsa(secp256k1::ecdsa::Signature),
}

#[derive(Clone, Hash, PartialEq, Eq)]
enum PublicKey {
    Schnorr(secp256k1::XOnlyPublicKey),
    Ecdsa(secp256k1::PublicKey),
}

// TODO: Make it pub(crate)
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct SigCacheKey {
    signature: Signature,
    pub_key: PublicKey,
    message: secp256k1::Message,
}

enum ScriptSource<'a, T: VerifiableTransaction> {
    TxInput { tx: &'a T, input: &'a TransactionInput, id: usize, utxo_entry: &'a UtxoEntry, is_p2sh: bool },
    StandAloneScripts(Vec<&'a [u8]>),
}

pub struct TxScriptEngine<'a, T: VerifiableTransaction> {
    dstack: Stack,
    astack: Stack,

    script_source: ScriptSource<'a, T>,

    // Outer caches for quicker calculation
    // TODO:: make it compatible with threading
    reused_values: &'a mut SigHashReusedValues,
    sig_cache: &'a Cache<SigCacheKey, bool>,

    cond_stack: Vec<OpCond>, // Following if stacks, and whether it is running

    num_ops: i32,
}

fn parse_script<T: VerifiableTransaction>(
    script: &[u8],
) -> impl Iterator<Item = Result<Box<dyn OpCodeImplementation<T>>, TxScriptError>> + '_ {
    script.iter().batching(|it| {
        // reads the opcode num item here and then match to opcode
        it.next().map(|code| deserialize_opcode_data(*code, it))
    })
}

pub fn get_sig_op_count<T: VerifiableTransaction>(signature_script: &[u8], prev_script_public_key: &ScriptPublicKey) -> u64 {
    let is_p2sh = ScriptClass::is_pay_to_script_hash(prev_script_public_key.script());
    let script_pub_key_ops = parse_script::<T>(prev_script_public_key.script()).collect_vec();
    if !is_p2sh {
        return get_sig_op_count_by_opcodes(&script_pub_key_ops);
    }

    let signature_script_ops = parse_script::<T>(signature_script).collect_vec();
    if signature_script_ops.is_empty() || signature_script_ops.iter().any(|op| op.is_err() || !op.as_ref().unwrap().is_push_opcode()) {
        return 0;
    }

    let p2sh_script = signature_script_ops.last().expect("checked if empty above").as_ref().expect("checked if err above").get_data();
    let p2sh_ops = parse_script::<T>(p2sh_script).collect_vec();
    get_sig_op_count_by_opcodes(&p2sh_ops)
}

fn get_sig_op_count_by_opcodes<T: VerifiableTransaction>(opcodes: &[Result<Box<dyn OpCodeImplementation<T>>, TxScriptError>]) -> u64 {
    // TODO: Check for overflows
    let mut num_sigs: u64 = 0;
    for (i, op) in opcodes.iter().enumerate() {
        match op {
            Ok(op) => {
                match op.value() {
                    codes::OpCheckSig | codes::OpCheckSigVerify | codes::OpCheckSigECDSA => num_sigs += 1,
                    codes::OpCheckMultiSig | codes::OpCheckMultiSigVerify | codes::OpCheckMultiSigECDSA => {
                        if i == 0 {
                            num_sigs += MAX_PUB_KEYS_PER_MUTLTISIG as u64;
                            continue;
                        }

                        let prev_opcode = opcodes[i - 1].as_ref().expect("they were checked before");
                        if prev_opcode.value() >= codes::OpTrue && prev_opcode.value() <= codes::Op16 {
                            num_sigs += to_small_int(prev_opcode) as u64;
                        } else {
                            num_sigs += MAX_PUB_KEYS_PER_MUTLTISIG as u64;
                        }
                    }
                    _ => {} // If the opcode is not a sigop, no need to increase the count
                }
            }
            Err(_) => return num_sigs,
        }
    }
    num_sigs
}

impl<'a, T: VerifiableTransaction> TxScriptEngine<'a, T> {
    pub fn new(reused_values: &'a mut SigHashReusedValues, sig_cache: &'a Cache<SigCacheKey, bool>) -> Self {
        Self {
            dstack: vec![],
            astack: vec![],
            script_source: ScriptSource::StandAloneScripts(vec![]),
            reused_values,
            sig_cache,
            cond_stack: vec![],
            num_ops: 0,
        }
    }

    pub fn from_transaction_input(
        tx: &'a T,
        input: &'a TransactionInput,
        input_idx: usize,
        utxo_entry: &'a UtxoEntry,
        reused_values: &'a mut SigHashReusedValues,
        sig_cache: &'a Cache<SigCacheKey, bool>,
    ) -> Result<Self, TxScriptError> {
        let script_public_key = utxo_entry.script_public_key.script();
        // The script_public_key in P2SH is just validating the hash on the OpMultiSig script
        // the user provides
        let is_p2sh = ScriptClass::is_pay_to_script_hash(script_public_key);
        match input_idx < tx.tx().inputs.len() {
            true => Ok(Self {
                dstack: Default::default(),
                astack: Default::default(),
                script_source: ScriptSource::TxInput { tx, input, id: input_idx, utxo_entry, is_p2sh },
                reused_values,
                sig_cache,
                cond_stack: Default::default(),
                num_ops: 0,
            }),
            false => Err(TxScriptError::InvalidIndex(input_idx, tx.tx().inputs.len())),
        }
    }

    pub fn from_script(script: &'a [u8], reused_values: &'a mut SigHashReusedValues, sig_cache: &'a Cache<SigCacheKey, bool>) -> Self {
        Self {
            dstack: Default::default(),
            astack: Default::default(),
            script_source: ScriptSource::StandAloneScripts(vec![script]),
            reused_values,
            sig_cache,
            cond_stack: Default::default(),
            num_ops: 0,
        }
    }

    #[inline]
    pub fn is_executing(&self) -> bool {
        return self.cond_stack.is_empty() || *self.cond_stack.last().expect("Checked not empty") == OpCond::True;
    }

    fn execute_opcode(&mut self, opcode: Box<dyn OpCodeImplementation<T>>) -> Result<(), TxScriptError> {
        // Different from kaspad: Illegal and disabled opcode are checked on execute instead
        // Note that this includes OP_RESERVED which counts as a push operation.
        if !opcode.is_push_opcode() {
            self.num_ops += 1;
            if self.num_ops > MAX_OPS_PER_SCRIPT {
                return Err(TxScriptError::TooManyOperations(MAX_OPS_PER_SCRIPT));
            }
        } else if opcode.len() > MAX_SCRIPT_ELEMENT_SIZE {
            return Err(TxScriptError::ElementTooBig(opcode.len(), MAX_SCRIPT_ELEMENT_SIZE));
        }

        if self.is_executing() || opcode.is_conditional() {
            if opcode.value() > 0 && opcode.value() <= 0x4e {
                opcode.check_minimal_data_push()?;
            }
            opcode.execute(self)
        } else {
            Ok(())
        }
    }

    fn execute_script(&mut self, script: &[u8], verify_only_push: bool) -> Result<(), TxScriptError> {
        let script_result = parse_script(script).try_for_each(|opcode| {
            let opcode = opcode?;
            if verify_only_push && !opcode.is_push_opcode() {
                return Err(TxScriptError::SignatureScriptNotPushOnly);
            }

            self.execute_opcode(opcode)?;

            let combined_size = self.astack.len() + self.dstack.len();
            if combined_size > MAX_STACK_SIZE {
                return Err(TxScriptError::StackSizeExceeded(combined_size, MAX_STACK_SIZE));
            }
            Ok(())
        });

        // Moving between scripts
        // TODO: Check that we are not in if when moving between scripts
        // Alt stack doesn't persist
        self.astack.clear();
        self.num_ops = 0; // number of ops is per script.

        script_result
    }

    pub fn execute(&mut self) -> Result<(), TxScriptError> {
        let (scripts, is_p2sh) = match &self.script_source {
            ScriptSource::TxInput { input, utxo_entry, is_p2sh, .. } => {
                if utxo_entry.script_public_key.version() > MAX_SCRIPT_PUBLIC_KEY_VERSION {
                    trace!("The version of the scriptPublicKey is higher than the known version - the Execute function returns true.");
                    return Ok(());
                }
                (vec![input.signature_script.as_slice(), utxo_entry.script_public_key.script()], *is_p2sh)
            }
            ScriptSource::StandAloneScripts(scripts) => (scripts.clone(), false),
        };

        // TODO: run all in same iterator?
        // When both the signature script and public key script are empty the
        // result is necessarily an error since the stack would end up being
        // empty which is equivalent to a false top element. Thus, just return
        // the relevant error now as an optimization.
        if scripts.is_empty() {
            return Err(TxScriptError::NoScripts);
        }

        if scripts.iter().all(|e| e.is_empty()) {
            return Err(TxScriptError::FalseStackEntry);
        }
        if scripts.iter().any(|e| e.len() > MAX_SCRIPTS_SIZE) {
            return Err(TxScriptError::FalseStackEntry);
        }

        let mut saved_stack: Option<Vec<Vec<u8>>> = None;
        // try_for_each quits only if an error occurred. So, we always run over all scripts if
        // each is successful
        scripts.iter().enumerate().filter(|(_, s)| !s.is_empty()).try_for_each(|(idx, s)| {
            let verify_only_push =
                idx == 0 && matches!(self.script_source, ScriptSource::TxInput { tx: _, input: _, id: _, utxo_entry: _, is_p2sh: _ });
            // Save script in p2sh
            if is_p2sh && idx == 1 {
                saved_stack = Some(self.dstack.clone());
            }
            self.execute_script(s, verify_only_push)
        })?;

        if is_p2sh {
            self.check_error_condition(false)?;
            self.dstack = saved_stack.ok_or(TxScriptError::EmptyStack)?;
            let script = self.dstack.pop().ok_or(TxScriptError::EmptyStack)?;
            self.execute_script(script.as_slice(), false)?
        }

        self.check_error_condition(true)?;
        Ok(())
    }

    // check_error_condition is called whenever we finish a chunk of the scripts
    // (all original scripts, all scripts including p2sh, and maybe future extensions)
    // returns Ok(()) if the running script has ended and was successful, leaving a a true boolean
    // on the stack. An error otherwise.
    #[inline]
    fn check_error_condition(&mut self, final_script: bool) -> Result<(), TxScriptError> {
        if final_script {
            if self.dstack.len() > 1 {
                return Err(TxScriptError::CleanStack(self.dstack.len() - 1));
            } else if self.dstack.is_empty() {
                return Err(TxScriptError::EmptyStack);
            }
        }

        let [v]: [bool; 1] = self.dstack.pop_items()?;
        match v {
            true => Ok(()),
            false => Err(TxScriptError::EvalFalse),
        }
    }

    // *** SIGNATURE SPECIFIC CODE **

    fn check_pub_key_encoding(pub_key: &[u8]) -> Result<(), TxScriptError> {
        match pub_key.len() {
            32 => Ok(()),
            _ => Err(TxScriptError::PubKeyFormat),
        }
    }

    fn check_pub_key_encoding_ecdsa(pub_key: &[u8]) -> Result<(), TxScriptError> {
        match pub_key.len() {
            33 => Ok(()),
            _ => Err(TxScriptError::PubKeyFormat),
        }
    }

    fn op_check_multisig_schnorr_or_ecdsa(&mut self, ecdsa: bool) -> Result<(), TxScriptError> {
        let [num_keys]: [i32; 1] = self.dstack.pop_items()?;
        if num_keys < 0 {
            return Err(TxScriptError::InvalidPubKeyCount(format!("number of pubkeys {num_keys} is negative")));
        } else if num_keys > MAX_PUB_KEYS_PER_MUTLTISIG {
            return Err(TxScriptError::InvalidPubKeyCount(format!("too many pubkeys {num_keys} > {MAX_PUB_KEYS_PER_MUTLTISIG}")));
        }
        let num_keys_usize = num_keys as usize;

        self.num_ops += num_keys;
        if self.num_ops > MAX_OPS_PER_SCRIPT {
            return Err(TxScriptError::TooManyOperations(MAX_OPS_PER_SCRIPT));
        }

        let pub_keys = match self.dstack.len() >= num_keys_usize {
            true => self.dstack.split_off(self.dstack.len() - num_keys_usize),
            false => return Err(TxScriptError::EmptyStack),
        };

        let [num_sigs]: [i32; 1] = self.dstack.pop_items()?;
        if num_sigs < 0 {
            return Err(TxScriptError::InvalidSignatureCount(format!("number of signatures {num_sigs} is negative")));
        } else if num_sigs > num_keys {
            return Err(TxScriptError::InvalidSignatureCount(format!("more signatures than pubkeys {num_sigs} > {num_keys}")));
        }
        let num_sigs = num_sigs as usize;

        let signatures = match self.dstack.len() >= num_sigs {
            true => self.dstack.split_off(self.dstack.len() - num_sigs),
            false => return Err(TxScriptError::EmptyStack),
        };

        let mut failed = false;
        let mut pub_key_iter = pub_keys.iter();
        'outer: for (sig_idx, signature) in signatures.iter().enumerate() {
            if signature.is_empty() {
                failed = true;
                break;
            }

            let typ = *signature.last().expect("checked that is not empty");
            let signature = &signature[..signature.len() - 1];
            let hash_type = SigHashType::from_u8(typ).map_err(|_| TxScriptError::InvalidSigHashType(typ))?;

            // Advance through the pub_keys iterator.
            // Note every check consumes the public key
            loop {
                if pub_key_iter.len() < num_sigs - sig_idx {
                    // When there are more signatures than public keys remaining,
                    // there is no way to succeed since too many signatures are
                    // invalid, so exit early.
                    failed = true;
                    break 'outer; // Break the outer signature loop
                }
                // SAFETY: we just checked the len
                let pub_key = pub_key_iter.next().unwrap();

                let check_signature_result = if ecdsa {
                    self.check_ecdsa_signature(hash_type, pub_key.as_slice(), signature)
                } else {
                    self.check_schnorr_signature(hash_type, pub_key.as_slice(), signature)
                };

                match check_signature_result {
                    Ok(valid) => {
                        if valid {
                            // Current sig is valid, we can break the inner loop and continue to next sig
                            break;
                        }
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        if failed && signatures.iter().any(|sig| !sig.is_empty()) {
            return Err(TxScriptError::NullFail);
        }

        self.dstack.push_item(!failed);
        Ok(())
    }

    #[inline]
    fn check_schnorr_signature(&mut self, hash_type: SigHashType, key: &[u8], sig: &[u8]) -> Result<bool, TxScriptError> {
        match self.script_source {
            ScriptSource::TxInput { tx, id, .. } => {
                if sig.len() != 64 {
                    return Err(TxScriptError::SigLength(sig.len()));
                }
                Self::check_pub_key_encoding(key)?;
                let pk = secp256k1::XOnlyPublicKey::from_slice(key).map_err(TxScriptError::InvalidSignature)?;
                let sig = secp256k1::schnorr::Signature::from_slice(sig).map_err(TxScriptError::InvalidSignature)?;
                let sig_hash = calc_schnorr_signature_hash(tx, id, hash_type, self.reused_values);
                let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
                let sig_cache_key =
                    SigCacheKey { signature: Signature::Secp256k1(sig), pub_key: PublicKey::Schnorr(pk), message: msg };

                match self.sig_cache.get(&sig_cache_key) {
                    Some(valid) => Ok(valid),
                    None => {
                        // TODO: Find a way to parallelize this part.
                        match sig.verify(&msg, &pk) {
                            Ok(()) => {
                                self.sig_cache.insert(sig_cache_key, true);
                                Ok(true)
                            }
                            Err(_) => {
                                self.sig_cache.insert(sig_cache_key, false);
                                Ok(false)
                            }
                        }
                    }
                }
            }
            _ => Err(TxScriptError::NotATransactionInput),
        }
    }

    fn check_ecdsa_signature(&mut self, hash_type: SigHashType, key: &[u8], sig: &[u8]) -> Result<bool, TxScriptError> {
        match self.script_source {
            ScriptSource::TxInput { tx, id, .. } => {
                if sig.len() != 64 {
                    return Err(TxScriptError::SigLength(sig.len()));
                }
                Self::check_pub_key_encoding_ecdsa(key)?;
                let pk = secp256k1::PublicKey::from_slice(key).map_err(TxScriptError::InvalidSignature)?;
                let sig = secp256k1::ecdsa::Signature::from_compact(sig).map_err(TxScriptError::InvalidSignature)?;
                let sig_hash = calc_ecdsa_signature_hash(tx, id, hash_type, self.reused_values);
                let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
                let sig_cache_key = SigCacheKey { signature: Signature::Ecdsa(sig), pub_key: PublicKey::Ecdsa(pk), message: msg };

                match self.sig_cache.get(&sig_cache_key) {
                    Some(valid) => Ok(valid),
                    None => {
                        // TODO: Find a way to parallelize this part.
                        match sig.verify(&msg, &pk) {
                            Ok(()) => {
                                self.sig_cache.insert(sig_cache_key, true);
                                Ok(true)
                            }
                            Err(_) => {
                                self.sig_cache.insert(sig_cache_key, false);
                                Ok(false)
                            }
                        }
                    }
                }
            }
            _ => Err(TxScriptError::NotATransactionInput),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::iter::once;

    use crate::opcodes::codes::{OpBlake2b, OpCheckSig, OpData1, OpData2, OpData32, OpDup, OpEqual, OpPushData1, OpTrue};

    use super::*;
    use consensus_core::tx::{
        PopulatedTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionOutpoint, TransactionOutput,
    };
    use smallvec::SmallVec;

    struct ScriptTestCase {
        script: &'static [u8],
        expected_result: Result<(), TxScriptError>,
    }

    struct KeyTestCase {
        name: &'static str,
        key: &'static [u8],
        is_valid: bool,
    }

    #[test]
    fn test_check_error_condition() {
        let test_cases = vec![
            ScriptTestCase {
                script: b"\x51", // opcodes::codes::OpTrue{data: ""}
                expected_result: Ok(()),
            },
            ScriptTestCase {
                script: b"\x61", // opcodes::codes::OpNop{data: ""}
                expected_result: Err(TxScriptError::EmptyStack),
            },
            ScriptTestCase {
                script: b"\x51\x51", // opcodes::codes::OpTrue, opcodes::codes::OpTrue,
                expected_result: Err(TxScriptError::CleanStack(1)),
            },
            ScriptTestCase {
                script: b"\x00", // opcodes::codes::OpFalse{data: ""},
                expected_result: Err(TxScriptError::EvalFalse),
            },
        ];

        let sig_cache = Cache::new(10_000);
        let mut reused_values = SigHashReusedValues::new();

        for test in test_cases {
            // Ensure encapsulation of variables (no leaking between tests)
            let input = TransactionInput {
                previous_outpoint: TransactionOutpoint {
                    transaction_id: TransactionId::from_bytes([
                        0xc9, 0x97, 0xa5, 0xe5, 0x6e, 0x10, 0x41, 0x02, 0xfa, 0x20, 0x9c, 0x6a, 0x85, 0x2d, 0xd9, 0x06, 0x60, 0xa2,
                        0x0b, 0x2d, 0x9c, 0x35, 0x24, 0x23, 0xed, 0xce, 0x25, 0x85, 0x7f, 0xcd, 0x37, 0x04,
                    ]),
                    index: 0,
                },
                signature_script: vec![],
                sequence: 4294967295,
                sig_op_count: 0,
            };
            let output = TransactionOutput { value: 1000000000, script_public_key: ScriptPublicKey::new(0, test.script.into()) };

            let tx = Transaction::new(1, vec![input.clone()], vec![output.clone()], 0, Default::default(), 0, vec![]);
            let utxo_entry = UtxoEntry::new(output.value, output.script_public_key.clone(), 0, tx.is_coinbase());

            let populated_tx = PopulatedTransaction::new(&tx, vec![utxo_entry.clone()]);

            let mut vm = TxScriptEngine::from_transaction_input(&populated_tx, &input, 0, &utxo_entry, &mut reused_values, &sig_cache)
                .expect("Script creation failed");
            assert_eq!(vm.execute(), test.expected_result);
        }
    }

    #[test]
    fn test_check_pub_key_encode() {
        let test_cases = vec![
            KeyTestCase {
                name: "uncompressed - invalid",
                key: &[
                    0x04u8, 0x11, 0xdb, 0x93, 0xe1, 0xdc, 0xdb, 0x8a, 0x01, 0x6b, 0x49, 0x84, 0x0f, 0x8c, 0x53, 0xbc, 0x1e, 0xb6,
                    0x8a, 0x38, 0x2e, 0x97, 0xb1, 0x48, 0x2e, 0xca, 0xd7, 0xb1, 0x48, 0xa6, 0x90, 0x9a, 0x5c, 0xb2, 0xe0, 0xea, 0xdd,
                    0xfb, 0x84, 0xcc, 0xf9, 0x74, 0x44, 0x64, 0xf8, 0x2e, 0x16, 0x0b, 0xfa, 0x9b, 0x8b, 0x64, 0xf9, 0xd4, 0xc0, 0x3f,
                    0x99, 0x9b, 0x86, 0x43, 0xf6, 0x56, 0xb4, 0x12, 0xa3,
                ],
                is_valid: false,
            },
            KeyTestCase {
                name: "compressed - invalid",
                key: &[
                    0x02, 0xce, 0x0b, 0x14, 0xfb, 0x84, 0x2b, 0x1b, 0xa5, 0x49, 0xfd, 0xd6, 0x75, 0xc9, 0x80, 0x75, 0xf1, 0x2e, 0x9c,
                    0x51, 0x0f, 0x8e, 0xf5, 0x2b, 0xd0, 0x21, 0xa9, 0xa1, 0xf4, 0x80, 0x9d, 0x3b, 0x4d,
                ],
                is_valid: false,
            },
            KeyTestCase {
                name: "compressed - invalid",
                key: &[
                    0x03, 0x26, 0x89, 0xc7, 0xc2, 0xda, 0xb1, 0x33, 0x09, 0xfb, 0x14, 0x3e, 0x0e, 0x8f, 0xe3, 0x96, 0x34, 0x25, 0x21,
                    0x88, 0x7e, 0x97, 0x66, 0x90, 0xb6, 0xb4, 0x7f, 0x5b, 0x2a, 0x4b, 0x7d, 0x44, 0x8e,
                ],
                is_valid: false,
            },
            KeyTestCase {
                name: "hybrid - invalid",
                key: &[
                    0x06, 0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x62, 0x95, 0xce, 0x87, 0x0b, 0x07, 0x02, 0x9b,
                    0xfc, 0xdb, 0x2d, 0xce, 0x28, 0xd9, 0x59, 0xf2, 0x81, 0x5b, 0x16, 0xf8, 0x17, 0x98, 0x48, 0x3a, 0xda, 0x77, 0x26,
                    0xa3, 0xc4, 0x65, 0x5d, 0xa4, 0xfb, 0xfc, 0x0e, 0x11, 0x08, 0xa8, 0xfd, 0x17, 0xb4, 0x48, 0xa6, 0x85, 0x54, 0x19,
                    0x9c, 0x47, 0xd0, 0x8f, 0xfb, 0x10, 0xd4, 0xb8,
                ],
                is_valid: false,
            },
            KeyTestCase {
                name: "32 bytes pubkey - Ok",
                key: &[
                    0x26, 0x89, 0xc7, 0xc2, 0xda, 0xb1, 0x33, 0x09, 0xfb, 0x14, 0x3e, 0x0e, 0x8f, 0xe3, 0x96, 0x34, 0x25, 0x21, 0x88,
                    0x7e, 0x97, 0x66, 0x90, 0xb6, 0xb4, 0x7f, 0x5b, 0x2a, 0x4b, 0x7d, 0x44, 0x8e,
                ],
                is_valid: true,
            },
            KeyTestCase { name: "empty", key: &[], is_valid: false },
        ];

        for test in test_cases {
            let check = TxScriptEngine::<PopulatedTransaction>::check_pub_key_encoding(test.key);
            if test.is_valid {
                assert_eq!(
                    check,
                    Ok(()),
                    "checkSignatureLength test '{}' failed when it should have succeeded: {:?}",
                    test.name,
                    check
                )
            } else {
                assert_eq!(
                    check,
                    Err(TxScriptError::PubKeyFormat),
                    "checkSignatureEncoding test '{}' succeeded or failed on wrong format ({:?})",
                    test.name,
                    check
                )
            }
        }
    }

    #[test]
    fn test_get_sig_op_count() {
        struct VerifiableTransactionMock {}

        impl VerifiableTransaction for VerifiableTransactionMock {
            fn tx(&self) -> &Transaction {
                todo!()
            }

            fn populated_input(&self, _index: usize) -> (&TransactionInput, &UtxoEntry) {
                todo!()
            }
        }

        struct TestVector<'a> {
            name: &'a str,
            signature_script: &'a [u8],
            expected_sig_ops: u64,
            prev_script_public_key: ScriptPublicKey,
        }

        let script_hash = hex::decode("433ec2ac1ffa1b7b7d027f564529c57197f9ae88").unwrap();
        let prev_script_pubkey_p2sh_script =
            [OpBlake2b, OpData32].iter().copied().chain(script_hash.iter().copied()).chain(once(OpEqual));
        let prev_script_pubkey_p2sh = ScriptPublicKey::new(0, SmallVec::from_iter(prev_script_pubkey_p2sh_script));

        let tests = [
            TestVector {
                name: "scriptSig doesn't parse",
                signature_script: &[OpPushData1, 0x02],
                expected_sig_ops: 0,
                prev_script_public_key: prev_script_pubkey_p2sh.clone(),
            },
            TestVector {
                name: "scriptSig isn't push only",
                signature_script: &[OpTrue, OpDup],
                expected_sig_ops: 0,
                prev_script_public_key: prev_script_pubkey_p2sh.clone(),
            },
            TestVector {
                name: "scriptSig length 0",
                signature_script: &[],
                expected_sig_ops: 0,
                prev_script_public_key: prev_script_pubkey_p2sh.clone(),
            },
            TestVector {
                name: "No script at the end",
                signature_script: &[OpTrue, OpTrue],
                expected_sig_ops: 0,
                prev_script_public_key: prev_script_pubkey_p2sh.clone(),
            }, // No script at end but still push only.
            TestVector {
                name: "pushed script doesn't parse",
                signature_script: &[OpData2, OpPushData1, 0x02],
                expected_sig_ops: 0,
                prev_script_public_key: prev_script_pubkey_p2sh,
            },
            TestVector {
                name: "mainnet multisig transaction 487f94ffa63106f72644068765b9dc629bb63e481210f382667d4a93b69af412",
                signature_script: &hex::decode("41eb577889fa28283709201ef5b056745c6cf0546dd31666cecd41c40a581b256e885d941b86b14d44efacec12d614e7fcabf7b341660f95bab16b71d766ab010501411c0eeef117ca485d34e4bc0cf6d5b578aa250c5d13ebff0882a7e2eeea1f31e8ecb6755696d194b1b0fcb853afab28b61f3f7cec487bd611df7e57252802f535014c875220ab64c7691713a32ea6dfced9155c5c26e8186426f0697af0db7a4b1340f992d12041ae738d66fe3d21105483e5851778ad73c5cddf0819c5e8fd8a589260d967e72065120722c36d3fac19646258481dd3661fa767da151304af514cb30af5cb5692203cd7690ecb67cbbe6cafad00a7c9133da535298ab164549e0cce2658f7b3032754ae").unwrap(),
                prev_script_public_key: ScriptPublicKey::new(
                    0,
                    SmallVec::from_slice(&hex::decode("aa20f38031f61ca23d70844f63a477d07f0b2c2decab907c2e096e548b0e08721c7987").unwrap()),
                ),
                expected_sig_ops: 4,
            },
            TestVector {
                name: "a partially parseable script public key",
                signature_script: &[],
                prev_script_public_key: ScriptPublicKey::new(
                    0,
                    SmallVec::from_slice(&[OpCheckSig,OpCheckSig, OpData1]),
                ),
                expected_sig_ops: 2,
            },
            TestVector {
                name: "p2pk",
                signature_script: &hex::decode("416db0c0ce824a6d076c8e73aae9987416933df768e07760829cb0685dc0a2bbb11e2c0ced0cab806e111a11cbda19784098fd25db176b6a9d7c93e5747674d32301").unwrap(),
                prev_script_public_key: ScriptPublicKey::new(
                    0,
                    SmallVec::from_slice(&hex::decode("208a457ca74ade0492c44c440da1cab5b008d8449150fe2794f0d8f4cce7e8aa27ac").unwrap()),
                ),
                expected_sig_ops: 1,
            },
        ];

        for test in tests {
            assert_eq!(
                get_sig_op_count::<VerifiableTransactionMock>(test.signature_script, &test.prev_script_public_key),
                test.expected_sig_ops,
                "failed for '{}'",
                test.name
            );
        }
    }
}
