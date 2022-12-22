extern crate alloc;
extern crate core;

pub mod caches;
mod opcodes;

use crate::caches::Cache;
use crate::opcodes::{deserialize, OpCodeImplementation};
use consensus_core::hashing::sighash::{calc_schnorr_signature_hash, SigHashReusedValues};
use consensus_core::hashing::sighash_type::SigHashType;
use consensus_core::tx::{PopulatedTransaction, TransactionInput, UtxoEntry};
use core::fmt::{Display, Formatter};
use itertools::Itertools;
use log::warn;

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

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum TxScriptError {
    // We return error if stack entry is false
    FalseStackEntry,
    InvalidIndex(usize, usize),
    StackSizeExceeded(usize),
    InvalidOpcode(String),
    OpcodeReserved(String),
    OpcodeDisabled(String),
    EmptyStack,
    EarlyReturn,
    VerifyError,
    InvalidState(String),
    InvalidSignature(secp256k1::Error),
    SigcacheSignatureInvalid,
    TooManyOperations(i32),
    NotATransactionInput,
    ElementTooBig(usize),
    NotMinimalData(String),
    InvalidSource(String),
    UnsatisfiedLockTime(String),
    NumberTooBig(String),
    NullFail,
    InvalidSignatureCount(String),
    InvalidPubKeyCount(String),
    InvalidSigHashType(String),
}

// TODO: Make it pub(crate)
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct SigCacheKey {
    signature: secp256k1::schnorr::Signature,
    pub_key: secp256k1::XOnlyPublicKey,
    message: secp256k1::Message,
}

impl Display for TxScriptError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Address decoding failed: {}",
            match self {
                Self::FalseStackEntry => "false stack entry at end of script execution".to_string(),
                Self::StackSizeExceeded(size) => format!("combined stack size {} > max allowed {}", size, MAX_STACK_SIZE),
                Self::InvalidOpcode(name) => format!("attempt to execute invalid opcode {}", name),
                Self::OpcodeReserved(name) => format!("attempt to execute reserved opcode {}", name),
                Self::OpcodeDisabled(name) => format!("attempt to execute disabled opcode {}", name),
                Self::EmptyStack => "attempt to read from empty stack".to_string(),
                Self::EarlyReturn => "script returned early".to_string(),
                Self::VerifyError => "script ran, but verification failed".to_string(),
                Self::InvalidState(s) => format!("encountered invalid state while running script: {}", s),
                Self::TooManyOperations(limit) => format!("exceeded max operation limit of {}", limit),
                Self::NotATransactionInput => "Engine is not running on a transaction input".to_string(),
                Self::InvalidSignature(e) => format!("signature invalid: {}", e),
                Self::SigcacheSignatureInvalid => "invalid signature in sig cache".to_string(),
                Self::InvalidIndex(id, tx_len) => format!("transaction input index {} >= {}", id, tx_len),
                Self::ElementTooBig(size) => format!("element size {} exceeds max allowed size {}", size, MAX_SCRIPT_ELEMENT_SIZE),
                Self::NotMinimalData(s) => format!("push encoding is not minimal: {}", s),
                Self::InvalidSource(s) => format!("opcode not supported on current source: {}", s),
                Self::UnsatisfiedLockTime(s) => format!("Unsatisfied lock time: {}", s),
                Self::NumberTooBig(s) => format!("Number too big: {}", s),
                Self::NullFail => "not all signatures empty on failed checkmultisig".to_string(),
                Self::InvalidSignatureCount(s) => format!("invalid signature count: {}", s),
                Self::InvalidPubKeyCount(s) => format!("invalid pubkey count: {}", s),
                Self::InvalidSigHashType(s) => format!("what's here {}", s),
            }
        )
    }
}

type Stack = Vec<Vec<u8>>;

enum ScriptSource<'a> {
    TxInput { tx: &'a PopulatedTransaction<'a>, input: &'a TransactionInput, id: usize, utxo_entry: &'a UtxoEntry },
    StandAloneScripts(Vec<&'a [u8]>),
}

pub struct TxScriptEngine<'a> {
    dstack: Stack,
    astack: Stack,

    script_source: ScriptSource<'a>,

    // Outer caches for quicker calculation
    // TODO:: make it compatible with threading
    reused_values: &'a mut SigHashReusedValues,
    sig_cache: &'a Cache<SigCacheKey, Result<(), secp256k1::Error>>,

    cond_stack: Vec<i8>, // Following if stacks, and whether it is running

    num_ops: i32,
}

impl<'a> TxScriptEngine<'a> {
    pub fn from_transaction_input(
        tx: &'a PopulatedTransaction<'a>,
        input: &'a TransactionInput,
        id: usize,
        utxo_entry: &'a UtxoEntry,
        reused_values: &'a mut SigHashReusedValues,
        sig_cache: &'a Cache<SigCacheKey, Result<(), secp256k1::Error>>,
    ) -> Result<Self, TxScriptError> {
        match id < tx.tx.inputs.len() {
            true => Ok(Self {
                dstack: Default::default(),
                astack: Default::default(),
                script_source: ScriptSource::TxInput { tx, input, id, utxo_entry },
                reused_values,
                sig_cache,
                cond_stack: Default::default(),
                num_ops: 0,
            }),
            false => Err(TxScriptError::InvalidIndex(id, tx.tx.inputs.len())),
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

    fn execute_opcode(&mut self, opcode: Box<dyn OpCodeImplementation>) -> Result<(), TxScriptError> {
        // Different from kaspad: Illegal and disabled opcode are checked on execute instead

        // Note that this includes OP_RESERVED which counts as a push operation.
        if opcode.value() > NO_COST_OPCODE {
            self.num_ops += 1;
            if self.num_ops > MAX_OPS_PER_SCRIPT {
                return Err(TxScriptError::TooManyOperations(MAX_OPS_PER_SCRIPT));
            }
        } else if opcode.len() > MAX_SCRIPT_ELEMENT_SIZE {
            return Err(TxScriptError::ElementTooBig(opcode.len()));
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
                    return Err(TxScriptError::StackSizeExceeded(combined_size));
                }
                Ok(())
            });

        // Moving between scripts
        // TODO: Check that we are not in if when moving between scripts
        // Alt stack doesn't persist
        self.astack.clear();
        self.num_ops = 0; // number of ops is per script.
                          // TODO: some checks for p2sh

        script_result
    }

    pub fn execute(&mut self) -> Result<(), TxScriptError> {
        let scripts = match &self.script_source {
            ScriptSource::TxInput { input, utxo_entry, .. } => {
                if utxo_entry.script_public_key.version() > MAX_SCRIPT_PUBLIC_KEY_VERSION {
                    warn!("The version of the scriptPublicKey is higher than the known version - the Execute function returns true.");
                    return Ok(());
                }
                // TODO: check parsed prv script is push only
                // TODO: isScriptHash(script_pubkey.script)
                vec![input.signature_script.as_slice(), utxo_entry.script_public_key.script()]
            }
            ScriptSource::StandAloneScripts(scripts) => scripts.clone(),
        };

        // TODO: run all in same iterator?
        // When both the signature script and public key script are empty the
        // result is necessarily an error since the stack would end up being
        // empty which is equivalent to a false top element. Thus, just return
        // the relevant error now as an optimization.
        if scripts.iter().all(|e| e.is_empty()) {
            return Err(TxScriptError::FalseStackEntry);
        }
        if scripts.iter().any(|e| e.len() > MAX_SCRIPTS_SIZE) {
            return Err(TxScriptError::FalseStackEntry);
        }

        scripts.iter().filter(|s| !s.is_empty()).try_for_each(|s| self.execute_script(s))
    }

    #[inline]
    fn check_schnorr_signature(&mut self, hash_type: SigHashType, key: &[u8], sig: &[u8]) -> Result<(), TxScriptError> {
        match self.script_source {
            ScriptSource::TxInput { tx, id, .. } => {
                // TODO: will crash the node. We need to replace it with a proper script engine once it's ready.
                let pk = secp256k1::XOnlyPublicKey::from_slice(key).map_err(TxScriptError::InvalidSignature)?;
                let sig = secp256k1::schnorr::Signature::from_slice(sig).map_err(TxScriptError::InvalidSignature)?;
                let sig_hash = calc_schnorr_signature_hash(tx, id, hash_type, self.reused_values);
                let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
                let sig_cache_key = SigCacheKey { signature: sig, pub_key: pk, message: msg };

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

    #[test]
    fn test_nop() {
        let sig_cache = Cache::new(10_000);
        let mut reused_values = SigHashReusedValues::new();
        let a = vec![0x61u8];
        let mut engine = TxScriptEngine::from_script(a.as_slice(), &mut reused_values, &sig_cache);
        assert_eq!(engine.execute(), Ok(()));

        let a = vec![0x61u8, 0x61u8];
        let mut engine = TxScriptEngine::from_script(a.as_slice(), &mut reused_values, &sig_cache);
        assert_eq!(engine.execute(), Ok(()));
    }
}
