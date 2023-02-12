extern crate alloc;
extern crate core;

pub mod caches;
mod data_stack;
mod opcodes;

use crate::caches::Cache;
use crate::data_stack::{DataStack, Stack};
use crate::opcodes::{deserialize, OpCodeImplementation};
use consensus_core::hashing::sighash::{calc_ecdsa_signature_hash, calc_schnorr_signature_hash, SigHashReusedValues};
use consensus_core::hashing::sighash_type::SigHashType;
use consensus_core::tx::{TransactionInput, UtxoEntry, VerifiableTransaction};
use itertools::Itertools;
use log::warn;
use txscript_errors::TxScriptError;

pub const MAX_SCRIPT_PUBLIC_KEY_VERSION: u16 = 0;
pub const MAX_STACK_SIZE: usize = 244;
pub const MAX_SCRIPTS_SIZE: usize = 10000;
pub const MAX_SCRIPT_ELEMENT_SIZE: usize = 520;
pub const MAX_OPS_PER_SCRIPT: i32 = 201;
pub const MAX_TX_IN_SEQUENCE_NUM: u64 = u64::MAX;
pub const SEQUENCE_LOCK_TIME_DISABLED: u64 = 1 << 63;
pub const SEQUENCE_LOCK_TIME_MASK: u64 = 0x00000000ffffffff;
pub const LOCK_TIME_THRESHOLD: u64 = 500_000_000_000;
pub const MAX_PUB_KEYS_PER_MUTLTISIG: i32 = 20;

// The last opcode that does not count toward operations.
// Note that this includes OP_RESERVED which counts as a push operation.
pub const NO_COST_OPCODE: u8 = 16;

#[derive(Clone, Hash, PartialEq, Eq)]
enum Signature {
    Secp256k1(secp256k1::schnorr::Signature),
    Ecdsa(secp256k1::ecdsa::Signature),
}

#[derive(Clone, Hash, PartialEq, Eq)]
enum PublicKey {
    Secp256k1(secp256k1::XOnlyPublicKey),
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
    sig_cache: &'a Cache<SigCacheKey, Result<(), secp256k1::Error>>,

    cond_stack: Vec<i8>, // Following if stacks, and whether it is running

    num_ops: i32,
}

impl<'a, T: VerifiableTransaction> TxScriptEngine<'a, T> {
    pub fn new(reused_values: &'a mut SigHashReusedValues, sig_cache: &'a Cache<SigCacheKey, Result<(), secp256k1::Error>>) -> Self {
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
        id: usize,
        utxo_entry: &'a UtxoEntry,
        reused_values: &'a mut SigHashReusedValues,
        sig_cache: &'a Cache<SigCacheKey, Result<(), secp256k1::Error>>,
    ) -> Result<Self, TxScriptError> {
        let pubkey_script = utxo_entry.script_public_key.script();
        // The pubkey in P2SH is just validating the hash on the OpMultiSig script
        // the user provides
        let is_p2sh = (pubkey_script.len() == 35) && // 3 opcodes number + 32 data
                (pubkey_script[0] == opcodes::codes::OpBlake2b) &&
                (pubkey_script[1] == opcodes::codes::OpData32) &&
                (pubkey_script[pubkey_script.len() -1] == opcodes::codes::OpEqual);
        match id < tx.tx().inputs.len() {
            true => Ok(Self {
                dstack: Default::default(),
                astack: Default::default(),
                script_source: ScriptSource::TxInput { tx, input, id, utxo_entry, is_p2sh },
                reused_values,
                sig_cache,
                cond_stack: Default::default(),
                num_ops: 0,
            }),
            false => Err(TxScriptError::InvalidIndex(id, tx.tx().inputs.len())),
        }
    }

    pub fn from_script(
        script: &'a [u8],
        reused_values: &'a mut SigHashReusedValues,
        sig_cache: &'a Cache<SigCacheKey, Result<(), secp256k1::Error>>,
    ) -> Self {
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
        // TODO: check values
        return self.cond_stack.is_empty() || *self.cond_stack.first().expect("Checked not empty") != 1;
    }

    fn execute_opcode(&mut self, opcode: Box<dyn OpCodeImplementation<T>>) -> Result<(), TxScriptError> {
        // Different from kaspad: Illegal and disabled opcode are checked on execute instead
        // Note that this includes OP_RESERVED which counts as a push operation.
        if opcode.value() > NO_COST_OPCODE {
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

    fn execute_script(&mut self, script: &[u8]) -> Result<(), TxScriptError> {
        let script_result = script
            .iter()
            .batching(|it| {
                // reads the opcode num item here and then match to opcode
                it.next().map(|code| deserialize(*code, it))
            })
            .try_for_each(|opcode| {
                self.execute_opcode(opcode?)?;

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
                    warn!("The version of the scriptPublicKey is higher than the known version - the Execute function returns true.");
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
            // Save script in p2sh
            if is_p2sh && idx == 1 {
                saved_stack = Some(self.dstack.clone());
            }
            self.execute_script(s)
        })?;

        if is_p2sh {
            self.check_error_condition(false)?;
            self.dstack = saved_stack.ok_or(TxScriptError::EmptyStack)?;
            let script = self.dstack.pop().ok_or(TxScriptError::EmptyStack)?;
            self.execute_script(script.as_slice())?
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

    #[inline]
    fn check_schnorr_signature(&mut self, hash_type: SigHashType, key: &[u8], sig: &[u8]) -> Result<(), TxScriptError> {
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
                    SigCacheKey { signature: Signature::Secp256k1(sig), pub_key: PublicKey::Secp256k1(pk), message: msg };

                match self.sig_cache.get(&sig_cache_key) {
                    Some(valid) => valid.map_err(TxScriptError::InvalidSignature),
                    None => {
                        // TODO: Find a way to parallelize this part. This will be less trivial
                        // once this code is inside the script engine.
                        match sig.verify(&msg, &pk) {
                            Ok(()) => {
                                self.sig_cache.insert(sig_cache_key, Ok(()));
                                Ok(())
                            }
                            Err(e) => {
                                self.sig_cache.insert(sig_cache_key, Err(e));
                                Err(TxScriptError::InvalidSignature(e))
                            }
                        }
                    }
                }
            }
            _ => Err(TxScriptError::NotATransactionInput),
        }
    }

    fn check_ecdsa_signature(&mut self, hash_type: SigHashType, key: &[u8], sig: &[u8]) -> Result<(), TxScriptError> {
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
                    Some(valid) => valid.map_err(TxScriptError::InvalidSignature),
                    None => {
                        // TODO: Find a way to parallelize this part. This will be less trivial
                        // once this code is inside the script engine.
                        match sig.verify(&msg, &pk) {
                            Ok(()) => {
                                self.sig_cache.insert(sig_cache_key, Ok(()));
                                Ok(())
                            }
                            Err(e) => {
                                self.sig_cache.insert(sig_cache_key, Err(e));
                                Err(TxScriptError::InvalidSignature(e))
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
    use super::*;
    use consensus_core::tx::{
        PopulatedTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionOutpoint, TransactionOutput,
    };

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
}
