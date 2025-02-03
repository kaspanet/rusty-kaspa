extern crate alloc;
extern crate core;

pub mod caches;
mod data_stack;
pub mod error;
pub mod opcodes;
pub mod result;
pub mod script_builder;
pub mod script_class;
pub mod standard;
#[cfg(feature = "wasm32-sdk")]
pub mod wasm;

pub mod runtime_sig_op_counter;

use crate::caches::Cache;
use crate::data_stack::{DataStack, Stack};
use crate::opcodes::{deserialize_next_opcode, OpCodeImplementation};
use itertools::Itertools;
use kaspa_consensus_core::hashing::sighash::{
    calc_ecdsa_signature_hash, calc_schnorr_signature_hash, SigHashReusedValues, SigHashReusedValuesUnsync,
};
use kaspa_consensus_core::hashing::sighash_type::SigHashType;
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionInput, UtxoEntry, VerifiableTransaction};
use kaspa_txscript_errors::TxScriptError;
use log::trace;
use opcodes::codes::OpReturn;
use opcodes::{codes, to_small_int, OpCond};
use script_class::ScriptClass;

pub mod prelude {
    pub use super::standard::*;
}
use crate::runtime_sig_op_counter::{RuntimeSigOpCounter, SigOpConsumer};
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

type DynOpcodeImplementation<Tx, Reused> = Box<dyn OpCodeImplementation<Tx, Reused>>;

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
    TxInput { tx: &'a T, input: &'a TransactionInput, idx: usize, utxo_entry: &'a UtxoEntry, is_p2sh: bool },
    StandAloneScripts(Vec<&'a [u8]>),
}

pub struct TxScriptEngine<'a, T: VerifiableTransaction, Reused: SigHashReusedValues> {
    dstack: Stack,
    astack: Stack,

    script_source: ScriptSource<'a, T>,

    // Outer caches for quicker calculation
    reused_values: &'a Reused,
    sig_cache: &'a Cache<SigCacheKey, bool>,

    cond_stack: Vec<OpCond>, // Following if stacks, and whether it is running

    num_ops: i32,
    kip10_enabled: bool,
    runtime_sig_op_counter: Option<RuntimeSigOpCounter>,
}

fn parse_script<T: VerifiableTransaction, Reused: SigHashReusedValues>(
    script: &[u8],
) -> impl Iterator<Item = Result<DynOpcodeImplementation<T, Reused>, TxScriptError>> + '_ {
    script.iter().batching(|it| deserialize_next_opcode(it))
}

/// Determines the exact number of signature operations executed in a transaction input
/// by simulating the script execution. Takes into account conditional branches and only
/// counts signature operations that are actually executed.
///
/// Example of how counts differ:
/// ```text
/// IF
///     CHECKSIG        // 1 sig op if true branch taken
/// ELSE
///     CHECKSIG        // 3 sig ops if false branch taken
///     CHECKSIG
///     CHECKSIG
/// ENDIF
/// ```
/// `get_sig_op_upper_bound` would return 4, while this function returns 1 or 3
/// depending on which branch is actually executed.
///
/// This function should be used:
/// - After the runtime signature operation counting hardfork activation
/// - When exact sig op counts are needed for fee calculation
/// - For accurate validation of sig op limits
/// - When working with scripts that have conditional logic
///
/// # Arguments
/// * `tx` - The transaction containing the input to analyze
/// * `input_idx` - Index of the input to analyze
/// * `kip10_enabled` - Whether KIP-10 features are enabled
///
/// # Returns
/// * `Ok(u8)` - The exact number of signature operations executed
/// * `Err(TxScriptError)` - If script execution fails or input index is invalid
pub fn get_sig_op_count<T: VerifiableTransaction>(tx: &T, input_idx: usize, kip10_enabled: bool) -> Result<u8, TxScriptError> {
    let sig_cache = Cache::new(0);
    let reused_values = SigHashReusedValuesUnsync::new();
    let mut vm = TxScriptEngine::from_transaction_input(
        tx,
        &tx.inputs()[input_idx],
        input_idx,
        tx.utxo(input_idx).ok_or_else(|| TxScriptError::InvalidInputIndex(input_idx as i32, tx.inputs().len()))?,
        &reused_values,
        &sig_cache,
        kip10_enabled,
        true,
    );
    vm.execute()?;
    Ok(vm.used_sig_ops().unwrap())
}

/// Calculates an upper bound of signature operations in a script without executing it.
/// This is faster than `get_sig_op_count` but may overestimate the count in scripts
/// with conditional logic.
///
/// This function should be used:
/// - Before the runtime signature operation counting hardfork activation
/// - When you need a conservative upper bound for validation
/// - When fast static analysis is preferred over exact counting
/// - For preliminary transaction size and fee estimation
///
/// # Arguments
/// * `signature_script` - The signature script to analyze
/// * `prev_script_public_key` - The previous output's script public key
///
/// # Returns
/// * `u64` - Upper bound of possible signature operations in the script
#[must_use]
pub fn get_sig_op_count_upper_bound<T: VerifiableTransaction, Reused: SigHashReusedValues>(
    signature_script: &[u8],
    prev_script_public_key: &ScriptPublicKey,
) -> u64 {
    let is_p2sh = ScriptClass::is_pay_to_script_hash(prev_script_public_key.script());
    let script_pub_key_ops = parse_script::<T, Reused>(prev_script_public_key.script()).collect_vec();
    if !is_p2sh {
        return get_sig_op_count_by_opcodes(&script_pub_key_ops);
    }

    let signature_script_ops = parse_script::<T, Reused>(signature_script).collect_vec();
    if signature_script_ops.is_empty() || signature_script_ops.iter().any(|op| op.is_err() || !op.as_ref().unwrap().is_push_opcode()) {
        return 0;
    }

    let p2sh_script = signature_script_ops.last().expect("checked if empty above").as_ref().expect("checked if err above").get_data();
    let p2sh_ops = parse_script::<T, Reused>(p2sh_script).collect_vec();
    get_sig_op_count_by_opcodes(&p2sh_ops)
}

fn get_sig_op_count_by_opcodes<T: VerifiableTransaction, Reused: SigHashReusedValues>(
    opcodes: &[Result<DynOpcodeImplementation<T, Reused>, TxScriptError>],
) -> u64 {
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

/// Returns whether the passed public key script is unspendable, or guaranteed to fail at execution.
///
/// This allows inputs to be pruned instantly when entering the UTXO set.
pub fn is_unspendable<T: VerifiableTransaction, Reused: SigHashReusedValues>(script: &[u8]) -> bool {
    parse_script::<T, Reused>(script).enumerate().any(|(index, op)| op.is_err() || (index == 0 && op.unwrap().value() == OpReturn))
}

impl<'a, T: VerifiableTransaction, Reused: SigHashReusedValues> TxScriptEngine<'a, T, Reused> {
    pub fn new(reused_values: &'a Reused, sig_cache: &'a Cache<SigCacheKey, bool>, kip10_enabled: bool) -> Self {
        Self {
            dstack: vec![],
            astack: vec![],
            script_source: ScriptSource::StandAloneScripts(vec![]),
            reused_values,
            sig_cache,
            cond_stack: vec![],
            num_ops: 0,
            kip10_enabled,
            runtime_sig_op_counter: None,
        }
    }

    /// Returns the number of signature operations used in script execution if runtime sig op counting is enabled.
    ///
    /// Returns None if runtime signature operation counting is disabled.
    pub fn used_sig_ops(&self) -> Option<u8> {
        self.runtime_sig_op_counter.as_ref().map(|counter| counter.used_sig_ops())
    }

    /// Creates a new Script Engine for validating transaction input.
    ///
    /// # Arguments
    /// * `tx` - The transaction being validated
    /// * `input` - The input being validated
    /// * `input_idx` - Index of the input in the transaction
    /// * `utxo_entry` - UTXO entry being spent
    /// * `reused_values` - Reused values for signature hashing
    /// * `sig_cache` - Cache for signature verification
    /// * `kip10_enabled` - Whether KIP-10 transaction introspection opcodes are enabled
    ///
    /// # Panics
    /// * When input_idx >= number of inputs in transaction (malformed input)
    ///
    /// # Returns
    /// Script engine instance configured for the given input
    pub fn from_transaction_input(
        tx: &'a T,
        input: &'a TransactionInput,
        input_idx: usize,
        utxo_entry: &'a UtxoEntry,
        reused_values: &'a Reused,
        sig_cache: &'a Cache<SigCacheKey, bool>,
        kip10_enabled: bool,
        runtime_sig_op_counting: bool,
    ) -> Self {
        let script_public_key = utxo_entry.script_public_key.script();
        // The script_public_key in P2SH is just validating the hash on the OpMultiSig script
        // the user provides
        let is_p2sh = ScriptClass::is_pay_to_script_hash(script_public_key);
        assert!(input_idx < tx.tx().inputs.len());
        Self {
            dstack: Default::default(),
            astack: Default::default(),
            script_source: ScriptSource::TxInput { tx, input, idx: input_idx, utxo_entry, is_p2sh },
            reused_values,
            sig_cache,
            cond_stack: Default::default(),
            num_ops: 0,
            kip10_enabled,
            runtime_sig_op_counter: runtime_sig_op_counting.then_some(RuntimeSigOpCounter::new(input.sig_op_count)),
        }
    }

    pub fn from_script(
        script: &'a [u8],
        reused_values: &'a Reused,
        sig_cache: &'a Cache<SigCacheKey, bool>,
        kip10_enabled: bool,
    ) -> Self {
        Self {
            dstack: Default::default(),
            astack: Default::default(),
            script_source: ScriptSource::StandAloneScripts(vec![script]),
            reused_values,
            sig_cache,
            cond_stack: Default::default(),
            num_ops: 0,
            kip10_enabled,
            // Runtime sig op counting is not needed for standalone scripts, only inputs have sig op count value
            runtime_sig_op_counter: None,
        }
    }

    #[inline]
    pub fn is_executing(&self) -> bool {
        self.cond_stack.is_empty() || *self.cond_stack.last().expect("Checked not empty") == OpCond::True
    }

    fn execute_opcode(&mut self, opcode: DynOpcodeImplementation<T, Reused>) -> Result<(), TxScriptError> {
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
            if opcode.is_disabled() {
                return Err(TxScriptError::OpcodeDisabled(format!("{:?}", opcode)));
            }

            if opcode.always_illegal() {
                return Err(TxScriptError::OpcodeReserved(format!("{:?}", opcode)));
            }

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

        // Moving between scripts - we can't be inside an if
        if script_result.is_ok() && !self.cond_stack.is_empty() {
            return Err(TxScriptError::ErrUnbalancedConditional);
        }

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
            return Err(TxScriptError::EvalFalse);
        }
        if let Some(s) = scripts.iter().find(|e| e.len() > MAX_SCRIPTS_SIZE) {
            return Err(TxScriptError::ScriptSize(s.len(), MAX_SCRIPTS_SIZE));
        }

        let mut saved_stack: Option<Vec<Vec<u8>>> = None;
        // try_for_each quits only if an error occurred. So, we always run over all scripts if
        // each is successful
        scripts.iter().enumerate().filter(|(_, s)| !s.is_empty()).try_for_each(|(idx, s)| {
            let verify_only_push =
                idx == 0 && matches!(self.script_source, ScriptSource::TxInput { tx: _, input: _, idx: _, utxo_entry: _, is_p2sh: _ });
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
    // returns Ok(()) if the running script has ended and was successful, leaving a true boolean
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
            false => return Err(TxScriptError::InvalidStackOperation(num_keys_usize, self.dstack.len())),
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
            false => return Err(TxScriptError::InvalidStackOperation(num_sigs, self.dstack.len())),
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

        self.dstack.push_item(!failed)?;
        Ok(())
    }

    #[inline]
    fn check_schnorr_signature(&mut self, hash_type: SigHashType, key: &[u8], sig: &[u8]) -> Result<bool, TxScriptError> {
        self.runtime_sig_op_counter.consume_sig_op()?;
        match self.script_source {
            ScriptSource::TxInput { tx, idx, .. } => {
                if sig.len() != 64 {
                    return Err(TxScriptError::SigLength(sig.len()));
                }
                Self::check_pub_key_encoding(key)?;
                let pk = secp256k1::XOnlyPublicKey::from_slice(key).map_err(TxScriptError::InvalidSignature)?;
                let sig = secp256k1::schnorr::Signature::from_slice(sig).map_err(TxScriptError::InvalidSignature)?;
                let sig_hash = calc_schnorr_signature_hash(tx, idx, hash_type, self.reused_values);
                let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();
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
        self.runtime_sig_op_counter.consume_sig_op()?;
        match self.script_source {
            ScriptSource::TxInput { tx, idx, .. } => {
                if sig.len() != 64 {
                    return Err(TxScriptError::SigLength(sig.len()));
                }
                Self::check_pub_key_encoding_ecdsa(key)?;
                let pk = secp256k1::PublicKey::from_slice(key).map_err(TxScriptError::InvalidSignature)?;
                let sig = secp256k1::ecdsa::Signature::from_compact(sig).map_err(TxScriptError::InvalidSignature)?;
                let sig_hash = calc_ecdsa_signature_hash(tx, idx, hash_type, self.reused_values);
                let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();
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

trait SpkEncoding {
    fn to_bytes(&self) -> Vec<u8>;
}

impl SpkEncoding for ScriptPublicKey {
    fn to_bytes(&self) -> Vec<u8> {
        self.version.to_be_bytes().into_iter().chain(self.script().iter().copied()).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::iter::once;

    use crate::opcodes::codes::{
        OpBlake2b, OpCheckMultiSig, OpCheckSig, OpCheckSigECDSA, OpCheckSigVerify, OpData1, OpData2, OpData32, OpDup, OpEndIf,
        OpEqual, OpFalse, OpIf, OpPushData1, OpTrue, OpVerify,
    };

    use super::*;
    use crate::script_builder::{ScriptBuilder, ScriptBuilderResult};
    use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
    use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
    use kaspa_consensus_core::tx::{
        MutableTransaction, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionOutpoint, TransactionOutput,
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

    struct VerifiableTransactionMock {}

    impl VerifiableTransaction for VerifiableTransactionMock {
        fn tx(&self) -> &Transaction {
            unimplemented!()
        }

        fn populated_input(&self, _index: usize) -> (&TransactionInput, &UtxoEntry) {
            unimplemented!()
        }

        fn utxo(&self, _index: usize) -> Option<&UtxoEntry> {
            unimplemented!()
        }
    }

    fn run_test_script_cases(test_cases: Vec<ScriptTestCase>) {
        let sig_cache = Cache::new(10_000);
        let reused_values = SigHashReusedValuesUnsync::new();

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
            [false, true].into_iter().for_each(|kip10_enabled| {
                [false, true].into_iter().for_each(|runtime_sig_op_counting| {
                    let mut vm = TxScriptEngine::from_transaction_input(
                        &populated_tx,
                        &input,
                        0,
                        &utxo_entry,
                        &reused_values,
                        &sig_cache,
                        kip10_enabled,
                        runtime_sig_op_counting,
                    );
                    assert_eq!(vm.execute(), test.expected_result);
                });
            });
        }
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

        run_test_script_cases(test_cases)
    }

    #[test]
    fn test_check_opif() {
        let test_cases = vec![
            ScriptTestCase {
                script: b"\x63", // OpIf
                expected_result: Err(TxScriptError::EmptyStack),
            },
            ScriptTestCase {
                script: b"\x52\x63", // Op2, OpIf - bool for If must be 0 or 1.
                expected_result: Err(TxScriptError::InvalidState("expected boolean".to_string())),
            },
            ScriptTestCase {
                script: b"\x51\x63", // OpTrue, OpIf
                expected_result: Err(TxScriptError::ErrUnbalancedConditional),
            },
            ScriptTestCase {
                script: b"\x00\x63", // OpFalse, OpIf
                expected_result: Err(TxScriptError::ErrUnbalancedConditional),
            },
            ScriptTestCase {
                script: b"\x51\x63\x51\x68", // OpTrue, OpIf, OpTrue, OpEndIf
                expected_result: Ok(()),
            },
            ScriptTestCase {
                script: b"\x00\x63\x51\x68", // OpFalse, OpIf, OpTrue, OpEndIf
                expected_result: Err(TxScriptError::EmptyStack),
            },
        ];

        run_test_script_cases(test_cases)
    }

    #[test]
    fn test_check_opelse() {
        let test_cases = vec![
            ScriptTestCase {
                script: b"\x67", // OpElse
                expected_result: Err(TxScriptError::InvalidState("condition stack empty".to_string())),
            },
            ScriptTestCase {
                script: b"\x51\x63\x67", // OpTrue, OpIf, OpElse
                expected_result: Err(TxScriptError::ErrUnbalancedConditional),
            },
            ScriptTestCase {
                script: b"\x00\x63\x67", // OpFalse, OpIf, OpElse
                expected_result: Err(TxScriptError::ErrUnbalancedConditional),
            },
            ScriptTestCase {
                script: b"\x51\x63\x51\x67\x68", // OpTrue, OpIf, OpTrue, OpElse, OpEndIf
                expected_result: Ok(()),
            },
            ScriptTestCase {
                script: b"\x00\x63\x67\x51\x68", // OpFalse, OpIf, OpElse, OpTrue, OpEndIf
                expected_result: Ok(()),
            },
        ];

        run_test_script_cases(test_cases)
    }

    #[test]
    fn test_check_opnotif() {
        let test_cases = vec![
            ScriptTestCase {
                script: b"\x64", // OpNotIf
                expected_result: Err(TxScriptError::EmptyStack),
            },
            ScriptTestCase {
                script: b"\x51\x64", // OpTrue, OpNotIf
                expected_result: Err(TxScriptError::ErrUnbalancedConditional),
            },
            ScriptTestCase {
                script: b"\x00\x64", // OpFalse, OpNotIf
                expected_result: Err(TxScriptError::ErrUnbalancedConditional),
            },
            ScriptTestCase {
                script: b"\x51\x64\x67\x51\x68", // OpTrue, OpNotIf, OpElse, OpTrue, OpEndIf
                expected_result: Ok(()),
            },
            ScriptTestCase {
                script: b"\x51\x64\x51\x67\x00\x68", // OpTrue, OpNotIf, OpTrue, OpElse, OpFalse, OpEndIf
                expected_result: Err(TxScriptError::EvalFalse),
            },
            ScriptTestCase {
                script: b"\x00\x64\x51\x68", // OpFalse, OpIf, OpTrue, OpEndIf
                expected_result: Ok(()),
            },
        ];

        run_test_script_cases(test_cases)
    }

    #[test]
    fn test_check_nestedif() {
        let test_cases = vec![
            ScriptTestCase {
                script: b"\x51\x63\x00\x67\x51\x63\x51\x68\x68", // OpTrue, OpIf, OpFalse, OpElse, OpTrue, OpIf,
                // OpTrue, OpEndIf, OpEndIf
                expected_result: Err(TxScriptError::EvalFalse),
            },
            ScriptTestCase {
                script: b"\x51\x63\x00\x67\x00\x63\x67\x51\x68\x68", // OpTrue, OpIf, OpFalse, OpElse, OpFalse, OpIf,
                // OpElse, OpTrue, OpEndIf, OpEndIf
                expected_result: Err(TxScriptError::EvalFalse),
            },
            ScriptTestCase {
                script: b"\x51\x64\x00\x67\x51\x63\x51\x68\x68", // OpTrue, OpNotIf, OpFalse, OpElse, OpTrue, OpIf,
                // OpTrue, OpEndIf, OpEndIf
                expected_result: Ok(()),
            },
            ScriptTestCase {
                script: b"\x51\x64\x00\x67\x00\x63\x67\x51\x68\x68", // OpTrue, OpNotIf, OpFalse, OpElse, OpFalse, OpIf,
                // OpTrue, OpEndIf, OpEndIf
                expected_result: Ok(()),
            },
            ScriptTestCase {
                script: b"\x51\x64\x00\x67\x00\x64\x00\x67\x51\x68\x68", // OpTrue, OpNotIf, OpFalse, OpElse, OpFalse, OpNotIf,
                // OpFalse, OpElse, OpTrue, OpEndIf, OpEndIf
                expected_result: Err(TxScriptError::EvalFalse),
            },
            ScriptTestCase {
                script: b"\x51\x00\x63\x63\x00\x68\x68", // OpTrue, OpFalse, OpIf, OpIf  OpFalse, OpEndIf, OpEndIf
                expected_result: Ok(()),
            },
            ScriptTestCase {
                script: b"\x51\x00\x63\x63\x63\x00\x67\x00\x68\x68\x68", // OpTrue, OpFalse, OpIf, OpIf  OpFalse, OpEndIf, OpEndIf
                expected_result: Ok(()),
            },
            ScriptTestCase {
                script: b"\x51\x00\x63\x63\x63\x63\x00\x67\x00\x68\x68\x68\x68", // OpTrue, OpFalse, OpIf, OpIf  OpFalse, OpEndIf, OpEndIf
                expected_result: Ok(()),
            },
        ];

        run_test_script_cases(test_cases)
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
            let check = TxScriptEngine::<PopulatedTransaction, SigHashReusedValuesUnsync>::check_pub_key_encoding(test.key);
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
                get_sig_op_count_upper_bound::<VerifiableTransactionMock, SigHashReusedValuesUnsync>(
                    test.signature_script,
                    &test.prev_script_public_key
                ),
                test.expected_sig_ops,
                "failed for '{}'",
                test.name
            );
        }
    }

    #[test]
    fn test_is_unspendable() {
        struct Test<'a> {
            name: &'a str,
            script_public_key: &'a [u8],
            expected: bool,
        }
        let tests = vec![
            Test { name: "unspendable", script_public_key: &[0x6a, 0x04, 0x74, 0x65, 0x73, 0x74], expected: true },
            Test {
                name: "spendable",
                script_public_key: &[
                    0x76, 0xa9, 0x14, 0x29, 0x95, 0xa0, 0xfe, 0x68, 0x43, 0xfa, 0x9b, 0x95, 0x45, 0x97, 0xf0, 0xdc, 0xa7, 0xa4, 0x4d,
                    0xf6, 0xfa, 0x0b, 0x5c, 0x88, 0xac,
                ],
                expected: false,
            },
        ];

        for test in tests {
            assert_eq!(
                is_unspendable::<VerifiableTransactionMock, SigHashReusedValuesUnsync>(test.script_public_key),
                test.expected,
                "failed for '{}'",
                test.name
            );
        }
    }

    #[derive(Clone)]
    struct SignatureData {
        signature: Vec<u8>,
        public_key: Vec<u8>,
    }

    /// Builder for constructing signature scripts with different signature types and combinations.
    enum SignatureScriptBuilder {
        /// Multisignature script that requires multiple signatures to be valid.
        Multisig(Vec<SignatureData>),

        /// Single signature script with one signature and its corresponding public key.
        Single(SignatureData),

        /// Mixed signature script that mix different signature types (e.g., ECDSA and Schnorr)
        Mixed(Vec<SignatureData>),

        /// Empty signature script builder
        None,
    }

    type SigBuilder = Box<dyn Fn(&MutableTransaction<Transaction>, &SigHashReusedValuesUnsync) -> SignatureScriptBuilder>;
    type ScriptBuilderFn = Box<dyn Fn(&mut ScriptBuilder) -> ScriptBuilderResult<&mut ScriptBuilder>>;

    struct TestCase {
        name: &'static str,
        script_builder: ScriptBuilderFn,
        sig_builder: SigBuilder,
        expected_sig_ops: u8,
        sig_op_limit: u8,
        should_pass: bool,
    }

    impl SignatureScriptBuilder {
        fn build(self, script: &[u8]) -> ScriptBuilderResult<Vec<u8>> {
            let mut builder = ScriptBuilder::new();

            match self {
                SignatureScriptBuilder::Single(sig_data) => {
                    builder.add_data(&sig_data.signature)?;
                    builder.add_data(&sig_data.public_key)?;
                }
                SignatureScriptBuilder::Multisig(sig_data_vec) => {
                    for sig_data in sig_data_vec {
                        builder.add_data(&sig_data.signature)?;
                    }
                }
                SignatureScriptBuilder::Mixed(sig_data_vec) => {
                    for sig_data in sig_data_vec {
                        builder.add_data(&sig_data.signature)?;
                        builder.add_data(&sig_data.public_key)?;
                    }
                }
                SignatureScriptBuilder::None => {}
            }

            builder.add_data(script)?;
            Ok(builder.drain())
        }
    }

    #[test]
    fn test_runtime_sig_op_count() -> ScriptBuilderResult<()> {
        // Setup keys and test environment
        let secp = secp256k1::Secp256k1::new();
        let (secret_key, _) = secp.generate_keypair(&mut rand::thread_rng());
        let keypair = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key.secret_bytes()).unwrap();

        let sig_cache = Cache::new(10_000);
        let reused_values = SigHashReusedValuesUnsync::new();

        // Helper functions for creating signatures
        let create_schnorr_signature = move |tx: &MutableTransaction<Transaction>, reused: &SigHashReusedValuesUnsync| {
            let hash = calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, reused);
            let msg = secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).unwrap();
            let sig = keypair.sign_schnorr(msg);
            let mut signature = sig.as_ref().to_vec();
            signature.push(SIG_HASH_ALL.to_u8());
            SignatureData { signature, public_key: keypair.x_only_public_key().0.serialize().to_vec() }
        };

        let create_ecdsa_signature = move |tx: &MutableTransaction<Transaction>, reused: &SigHashReusedValuesUnsync| {
            let hash = calc_ecdsa_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, reused);
            let msg = secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).unwrap();
            let sig = keypair.secret_key().sign_ecdsa(msg);
            let mut signature = sig.serialize_compact().to_vec();
            signature.push(SIG_HASH_ALL.to_u8());
            SignatureData { signature, public_key: keypair.public_key().serialize().to_vec() }
        };

        let test_cases = vec![
            // Basic Schnorr CheckSig
            TestCase {
                name: "Basic Schnorr CheckSig - Single signature",
                script_builder: Box::new(|sb| sb.add_op(OpCheckSig)),
                sig_builder: Box::new(move |tx, reused| SignatureScriptBuilder::Single(create_schnorr_signature(tx, reused))),
                expected_sig_ops: 1,
                sig_op_limit: 1,
                should_pass: true,
            },
            // Basic ECDSA CheckSig
            TestCase {
                name: "Basic ECDSA CheckSig - Single signature",
                script_builder: Box::new(|sb| sb.add_op(OpCheckSigECDSA)),
                sig_builder: Box::new(move |tx, reused| SignatureScriptBuilder::Single(create_ecdsa_signature(tx, reused))),
                expected_sig_ops: 1,
                sig_op_limit: 1,
                should_pass: true,
            },
            // Mixed Schnorr and ECDSA
            TestCase {
                name: "Mixed Schnorr and ECDSA - Within limit",
                script_builder: Box::new(|sb| sb.add_op(OpCheckSigVerify)?.add_op(OpCheckSigECDSA)),
                sig_builder: Box::new(move |tx, reused| {
                    SignatureScriptBuilder::Mixed(vec![create_ecdsa_signature(tx, reused), create_schnorr_signature(tx, reused)])
                }),
                expected_sig_ops: 2,
                sig_op_limit: 2,
                should_pass: true,
            },
            // 2-of-3 MultiSig test case
            TestCase {
                name: "2-of-3 MultiSig - Basic validation",
                script_builder: Box::new(move |sb| {
                    sb.add_i64(2)?
                        .add_data(&keypair.x_only_public_key().0.serialize())?
                        .add_data(&keypair.x_only_public_key().0.serialize())?
                        .add_data(&keypair.x_only_public_key().0.serialize())?
                        .add_i64(3)?
                        .add_op(OpCheckMultiSig)
                }),
                sig_builder: Box::new(move |tx, reused| {
                    let sig = create_schnorr_signature(tx, reused);
                    SignatureScriptBuilder::Multisig(vec![sig.clone(), sig])
                }),
                expected_sig_ops: 2,
                sig_op_limit: 2,
                should_pass: true,
            },
            TestCase {
                name: "Mixed Schnorr and ECDSA - Exceeds limit",
                script_builder: Box::new(|sb| sb.add_op(OpCheckSigVerify)?.add_op(OpCheckSigECDSA)),
                sig_builder: Box::new(move |tx, reused| {
                    SignatureScriptBuilder::Mixed(vec![create_ecdsa_signature(tx, reused), create_schnorr_signature(tx, reused)])
                }),
                expected_sig_ops: 2,
                sig_op_limit: 1,
                should_pass: false,
            },
            // Conditional execution with sig ops
            TestCase {
                name: "Conditional sig ops - True branch execution",
                script_builder: Box::new(|sb| sb.add_op(OpTrue)?.add_op(OpIf)?.add_op(OpCheckSigECDSA)?.add_op(OpEndIf)),
                sig_builder: Box::new(move |tx, reused| SignatureScriptBuilder::Single(create_ecdsa_signature(tx, reused))),
                expected_sig_ops: 1,
                sig_op_limit: 1,
                should_pass: true,
            },
            // Conditional execution with sig ops
            TestCase {
                name: "Conditional sig ops - False branch skips validation",
                script_builder: Box::new(|sb| {
                    sb.add_op(OpFalse)?.add_op(OpIf)?.add_op(OpCheckSigECDSA)?.add_op(OpVerify)?.add_op(OpEndIf)?.add_op(OpTrue)
                }),
                sig_builder: Box::new(move |_tx, _reused| SignatureScriptBuilder::None),
                expected_sig_ops: 0,
                sig_op_limit: 0,
                should_pass: true,
            },
        ];

        for test in test_cases {
            // Create script
            let mut script_builder = ScriptBuilder::new();
            (test.script_builder)(&mut script_builder)?;
            let script = script_builder.drain();

            let script_pub_key = pay_to_script_hash_script(&script);
            let utxo_entry = UtxoEntry::new(1000, script_pub_key.clone(), 0, false);

            // Create transaction
            let tx = Transaction::new(
                1,
                vec![TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: TransactionId::default(), index: 0 },
                    signature_script: vec![],
                    sequence: 0,
                    sig_op_count: test.sig_op_limit,
                }],
                vec![],
                0,
                Default::default(),
                0,
                vec![],
            );

            let mut tx = MutableTransaction::new(tx);
            tx.entries = vec![Some(utxo_entry.clone())];

            // Build signature script
            let signature_script = (test.sig_builder)(&tx, &reused_values).build(&script)?;
            tx.tx.inputs[0].signature_script = signature_script;

            // Execute script
            let tx = tx.as_verifiable();
            let mut vm =
                TxScriptEngine::from_transaction_input(&tx, &tx.inputs()[0], 0, &utxo_entry, &reused_values, &sig_cache, false, true);

            let result = vm.execute().map(|_| vm.used_sig_ops().unwrap());

            match (result, test.should_pass) {
                (Ok(count), true) => {
                    assert_eq!(
                        count, test.expected_sig_ops,
                        "{} failed: Expected {} sig ops, got {}",
                        test.name, test.expected_sig_ops, count
                    );
                }
                (Ok(_), false) => {
                    panic!("{} should have failed but succeeded", test.name);
                }
                (Err(err), true) => {
                    panic!("{} failed but should have succeeded with err: {}", test.name, err);
                }
                (Err(_), false) => {
                    // Test correctly failed
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod bitcoind_tests {
    // Bitcoind tests
    use serde::Deserialize;
    use std::fs::File;
    use std::io::BufReader;
    use std::path::Path;

    use super::*;
    use crate::script_builder::ScriptBuilderError;
    use kaspa_consensus_core::constants::MAX_TX_IN_SEQUENCE_NUM;
    use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
    use kaspa_consensus_core::tx::{
        PopulatedTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionOutpoint, TransactionOutput,
    };

    #[derive(PartialEq, Eq, Debug, Clone)]
    enum UnifiedError {
        TxScriptError(TxScriptError),
        ScriptBuilderError(ScriptBuilderError),
    }

    #[derive(PartialEq, Eq, Debug, Clone)]
    struct TestError {
        expected_result: String,
        result: Result<(), UnifiedError>,
    }

    #[allow(dead_code)]
    #[derive(Deserialize, Debug, Clone)]
    #[serde(untagged)]
    enum JsonTestRow {
        Test(String, String, String, String),
        TestWithComment(String, String, String, String, String),
        Comment((String,)),
    }

    fn create_spending_transaction(sig_script: Vec<u8>, script_public_key: ScriptPublicKey) -> Transaction {
        let coinbase = Transaction::new(
            1,
            vec![TransactionInput::new(
                TransactionOutpoint::new(TransactionId::default(), 0xffffffffu32),
                vec![0, 0],
                MAX_TX_IN_SEQUENCE_NUM,
                MAX_PUB_KEYS_PER_MUTLTISIG as u8,
            )],
            vec![TransactionOutput::new(0, script_public_key)],
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );

        Transaction::new(
            1,
            vec![TransactionInput::new(
                TransactionOutpoint::new(coinbase.id(), 0u32),
                sig_script,
                MAX_TX_IN_SEQUENCE_NUM,
                MAX_PUB_KEYS_PER_MUTLTISIG as u8,
            )],
            vec![TransactionOutput::new(0, Default::default())],
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        )
    }

    impl JsonTestRow {
        fn test_row(&self, kip10_enabled: bool, runtime_sig_op_counting: bool) -> Result<(), TestError> {
            // Parse test to objects
            let (sig_script, script_pub_key, expected_result) = match self.clone() {
                JsonTestRow::Test(sig_script, sig_pub_key, _, expected_result) => (sig_script, sig_pub_key, expected_result),
                JsonTestRow::TestWithComment(sig_script, sig_pub_key, _, expected_result, _) => {
                    (sig_script, sig_pub_key, expected_result)
                }
                JsonTestRow::Comment(_) => {
                    return Ok(());
                }
            };

            let result = Self::run_test(sig_script, script_pub_key, kip10_enabled, runtime_sig_op_counting);

            match Self::result_name(result.clone()).contains(&expected_result.as_str()) {
                true => Ok(()),
                false => Err(TestError { expected_result, result }),
            }
        }

        fn run_test(
            sig_script: String,
            script_pub_key: String,
            kip10_enabled: bool,
            runtime_sig_op_counting: bool,
        ) -> Result<(), UnifiedError> {
            let script_sig = opcodes::parse_short_form(sig_script).map_err(UnifiedError::ScriptBuilderError)?;
            let script_pub_key =
                ScriptPublicKey::from_vec(0, opcodes::parse_short_form(script_pub_key).map_err(UnifiedError::ScriptBuilderError)?);

            // Create transaction
            let tx = create_spending_transaction(script_sig, script_pub_key.clone());
            let entry = UtxoEntry::new(0, script_pub_key.clone(), 0, true);
            let populated_tx = PopulatedTransaction::new(&tx, vec![entry]);

            // Run transaction
            let sig_cache = Cache::new(10_000);
            let reused_values = SigHashReusedValuesUnsync::new();
            let mut vm = TxScriptEngine::from_transaction_input(
                &populated_tx,
                &populated_tx.tx().inputs[0],
                0,
                &populated_tx.entries[0],
                &reused_values,
                &sig_cache,
                kip10_enabled,
                runtime_sig_op_counting,
            );
            vm.execute().map_err(UnifiedError::TxScriptError)
        }

        /*

        // At this point an error was expected so ensure the result of
        // the execution matches it.
        success := false
        for _, code := range allowedErrorCodes {
            if IsErrorCode(err, code) {
                success = true
                break
            }
        }
        if !success {
            var scriptErr Error
            if ok := errors.As(err, &scriptErr); ok {
                t.Errorf("%s: want error codes %v, got %v", name,
                    allowedErrorCodes, scriptErr.ErrorCode)
                continue
            }
            t.Errorf("%s: want error codes %v, got err: %v (%T)",
                name, allowedErrorCodes, err, err)
            continue
        }*/

        fn result_name(result: Result<(), UnifiedError>) -> Vec<&'static str> {
            match result {
                Ok(_) => vec!["OK"],
                Err(ue) => match ue {
                    UnifiedError::TxScriptError(e) => match e {
                        TxScriptError::NumberTooBig(_) => vec!["UNKNOWN_ERROR"],
                        TxScriptError::Serialization(_) => vec!["UNKNOWN_ERROR"],
                        TxScriptError::PubKeyFormat => vec!["PUBKEYFORMAT"],
                        TxScriptError::EvalFalse => vec!["EVAL_FALSE"],
                        TxScriptError::EmptyStack => {
                            vec!["EMPTY_STACK", "EVAL_FALSE", "UNBALANCED_CONDITIONAL", "INVALID_ALTSTACK_OPERATION"]
                        }
                        TxScriptError::NullFail => vec!["NULLFAIL"],
                        TxScriptError::SigLength(_) => vec!["NULLFAIL"],
                        //SIG_HIGH_S
                        TxScriptError::InvalidSigHashType(_) => vec!["SIG_HASHTYPE"],
                        TxScriptError::SignatureScriptNotPushOnly => vec!["SIG_PUSHONLY"],
                        TxScriptError::CleanStack(_) => vec!["CLEANSTACK"],
                        TxScriptError::OpcodeReserved(_) => vec!["BAD_OPCODE"],
                        TxScriptError::MalformedPush(_, _) => vec!["BAD_OPCODE"],
                        TxScriptError::InvalidOpcode(_) => vec!["BAD_OPCODE"],
                        TxScriptError::ErrUnbalancedConditional => vec!["UNBALANCED_CONDITIONAL"],
                        TxScriptError::InvalidState(s) if s == "condition stack empty" => vec!["UNBALANCED_CONDITIONAL"],
                        //ErrInvalidStackOperation
                        TxScriptError::EarlyReturn => vec!["OP_RETURN"],
                        TxScriptError::VerifyError => vec!["VERIFY", "EQUALVERIFY"],
                        TxScriptError::InvalidStackOperation(_, _) => vec!["INVALID_STACK_OPERATION", "INVALID_ALTSTACK_OPERATION"],
                        TxScriptError::InvalidState(s) if s == "pick at an invalid location" => vec!["INVALID_STACK_OPERATION"],
                        TxScriptError::InvalidState(s) if s == "roll at an invalid location" => vec!["INVALID_STACK_OPERATION"],
                        TxScriptError::OpcodeDisabled(_) => vec!["DISABLED_OPCODE"],
                        TxScriptError::ElementTooBig(_, _) => vec!["PUSH_SIZE"],
                        TxScriptError::TooManyOperations(_) => vec!["OP_COUNT"],
                        TxScriptError::StackSizeExceeded(_, _) => vec!["STACK_SIZE"],
                        TxScriptError::InvalidPubKeyCount(_) => vec!["PUBKEY_COUNT"],
                        TxScriptError::InvalidSignatureCount(_) => vec!["SIG_COUNT"],
                        TxScriptError::NotMinimalData(_) => vec!["MINIMALDATA", "UNKNOWN_ERROR"],
                        //ErrNegativeLockTime
                        TxScriptError::UnsatisfiedLockTime(_) => vec!["UNSATISFIED_LOCKTIME"],
                        TxScriptError::InvalidState(s) if s == "expected boolean" => vec!["MINIMALIF"],
                        TxScriptError::ScriptSize(_, _) => vec!["SCRIPT_SIZE"],
                        _ => vec![],
                    },
                    UnifiedError::ScriptBuilderError(e) => match e {
                        ScriptBuilderError::ElementExceedsMaxSize(_) => vec!["PUSH_SIZE"],
                        _ => vec![],
                    },
                },
            }
        }
    }

    #[test]
    fn test_bitcoind_tests() {
        // Script test files are split into two versions to test behavior before and after KIP-10:
        //
        // - script_tests.json: Tests basic script functionality with KIP-10 disabled (kip10_enabled=false)
        // - script_tests-kip10.json: Tests expanded functionality with KIP-10 enabled (kip10_enabled=true)
        //
        // KIP-10 introduces two major changes:
        //
        // 1. Support for 8-byte integer arithmetic (previously limited to 4 bytes)
        //    This enables working with larger numbers in scripts and reduces artificial constraints
        //
        // 2. Transaction introspection opcodes:
        //    - OpTxInputCount (0xb3): Get number of inputs
        //    - OpTxOutputCount (0xb4): Get number of outputs
        //    - OpTxInputIndex (0xb9): Get current input index
        //    - OpTxInputAmount (0xbe): Get input amount
        //    - OpTxInputSpk (0xbf): Get input script public key
        //    - OpTxOutputAmount (0xc2): Get output amount
        //    - OpTxOutputSpk (0xc3): Get output script public key
        //
        // These changes were added to support mutual transactions and auto-compounding addresses.
        // When KIP-10 is disabled (pre-activation), the new opcodes will return an InvalidOpcode error
        // and arithmetic is limited to 4 bytes. When enabled, scripts gain full access to transaction
        // data and 8-byte arithmetic capabilities.
        for runtime_sig_op_counting in [false, true] {
            for (file_name, kip10_enabled) in [("script_tests.json", false), ("script_tests-kip10.json", true)] {
                let file = File::open(Path::new(env!("CARGO_MANIFEST_DIR")).join("test-data").join(file_name))
                    .expect("Could not find test file");
                let reader = BufReader::new(file);

                // Read the JSON contents of the file as an instance of `User`.
                let tests: Vec<JsonTestRow> = serde_json::from_reader(reader).expect("Failed Parsing {:?}");
                for row in tests {
                    if let Err(error) = row.test_row(kip10_enabled, runtime_sig_op_counting) {
                        panic!("Test: {:?} failed for {}: {:?}", row.clone(), file_name, error);
                    }
                }
            }
        }
    }
}
