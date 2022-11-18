extern crate core;
extern crate alloc;

mod opcodes;
pub mod caches;

use core::fmt::{Display, Formatter};
use consensus_core::tx::{PopulatedTransaction, ScriptPublicKey, TransactionInput, UtxoEntry};
use crate::opcodes::{OpCodeImplementation, deserialize};
use itertools::Itertools;
use log::warn;
use consensus_core::hashing::sighash::{calc_schnorr_signature_hash, SigHashReusedValues};
use consensus_core::hashing::sighash_type::SigHashType;
use crate::caches::Cache;


pub const MAX_SCRIPT_PUBLIC_KEY_VERSION: u16 = 0;
pub const MAX_STACK_SIZE: usize = 244;
pub const MAX_OPS_PER_SCRIPT: i32 = 201;
// The last opcode that does not count toward operations.
// Note that this includes OP_RESERVED which counts as a push operation.
pub const NO_COST_OPCODE: u8 = 16;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum TxScriptError {
    // We return error if stack entry is false
    FalseStackEntry,
    StackSizeExceeded(usize),
    OpcodeInvalid(String),
    OpcodeReserved(String),
    OpcodeDisabled(String),
    EmptyStack,
    EarlyReturn,
    VerifyError,
    InvalidState(String),
    SignatureInvalid(secp256k1::Error),
    SigcacheSignatureInvalid,
    TooManyOperations(i32),
    NotATransactionInput
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
                Self::FalseStackEntry => format!("false stack entry at end of script execution"),
                Self::StackSizeExceeded(size) => format!("combined stack size {} > max allowed {}", size, MAX_STACK_SIZE),
                Self::OpcodeInvalid(name) => format!("attempt to execute invalid opcode {}", name),
                Self::OpcodeReserved(name) => format!("attempt to execute reserved opcode {}", name),
                Self::OpcodeDisabled(name) => format!("attempt to execute disabled opcode {}", name),
                Self::EmptyStack => format!("attempt to read from empty stack"),
                Self::EarlyReturn => format!("script returned early"),
                Self::VerifyError => format!("script ran, but verification failed"),
                Self::InvalidState(s) => format!("encountered invalid state while running script: {}", s),
                Self::TooManyOperations(limit) => format!("exceeded max operation limit of {}", limit),
                Self::NotATransactionInput => format!("Engine is not running on a transaction input"),
                Self::SignatureInvalid(e) => format!("signature invalid: {}", e),
                Self::SigcacheSignatureInvalid => format!("invalid signature in sig cache")
            }
        )
    }
}

type Stack = Vec<Vec<u8>>;

enum ScriptSource<'a> {
    TxInput{
        tx: &'a PopulatedTransaction<'a>,
        input: &'a TransactionInput,
        id: usize,
        utxo_entry: &'a UtxoEntry
    },
    StandAloneScripts(Vec<&'a [u8]>)
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
    pub fn from_transaction_input(tx: &'a PopulatedTransaction<'a>, input: &'a TransactionInput, id: usize, utxo_entry: &'a UtxoEntry, reused_values: &'a mut SigHashReusedValues, sig_cache: &'a Cache<SigCacheKey, Result<(), secp256k1::Error>>) -> Self {
        Self{
            dstack: Default::default(),
            astack: Default::default(),
            script_source: ScriptSource::TxInput {tx, input, id, utxo_entry},
            reused_values,
            sig_cache,
            cond_stack: Default::default(),
            num_ops: 0
        }
    }

    pub fn from_script(script: &'a [u8], reused_values: &'a mut SigHashReusedValues, sig_cache: &'a Cache<SigCacheKey, Result<(), secp256k1::Error>>) -> Self {
        Self{
            dstack: Default::default(),
            astack: Default::default(),
            script_source: ScriptSource::StandAloneScripts(vec![script]),
            reused_values,
            sig_cache,
            cond_stack: Default::default(),
            num_ops: 0
        }
    }

    pub fn is_executing(&self) -> bool {
        todo!()
    }

    fn execute_opcode(&mut self, opcode: Box<dyn OpCodeImplementation>) -> Result<(), TxScriptError>{
        // TODO: check disabled
        // TODO: check illegal
        // Note that this includes OP_RESERVED which counts as a push operation.
        if opcode.value() > NO_COST_OPCODE {
            self.num_ops+=1;
            if self.num_ops > MAX_OPS_PER_SCRIPT {
                return Err(TxScriptError::TooManyOperations(MAX_OPS_PER_SCRIPT));
            }
        }

        // TODO: check if data is not too big
        // TODO: check if in brach?

        // TODO: check minimal data push
        // TODO: run opcode
        let a = format!("{:?}", opcode);
        opcode.execute(self)
    }

    pub fn execute_script(&mut self, script: &[u8]) -> Result<(), TxScriptError> {

        //let mut consumable = script.iter();
        script.iter().batching(|it| {
            // TODO: we need to read the opcode num item here and then match to opcode
            match it.next() {
                Some(code) => Some(deserialize(*code, it)),
                None => None
            }
        }).try_for_each(|opcode| {
            self.execute_opcode(opcode?)?;

            let combined_size = self.astack.len() + self.dstack.len();
            if combined_size > MAX_STACK_SIZE {
                return Err(TxScriptError::StackSizeExceeded(combined_size));
            }

            // Moving between scripts
            if true {
                // TODO: Check that we are not in if when moving between scripts
                // Alt stack doesn't persist
                self.astack.clear();

                // TODO: numops, scriptoff, scriptidx, something with p2sh

                // TODO: script zero scripts

                // TODO: exit when finished all scripts
            }
            Ok(())
        })
    }

    pub fn execute(&mut self) -> Result<(),TxScriptError> {
        let scripts = match &self.script_source {
            ScriptSource::TxInput {input, utxo_entry, .. } => {
                if utxo_entry.script_public_key.version() > MAX_SCRIPT_PUBLIC_KEY_VERSION {
                    warn!("The version of the scriptPublicKey is higher than the known version - the Execute function returns true.");
                    return Ok(());
                }
                // TODO: parseScriptAndVerifySize x2
                vec![input.signature_script.as_slice(), utxo_entry.script_public_key.script()]
            },
            ScriptSource::StandAloneScripts(scripts) => scripts.clone()
        };
        //TODO: removed a check on txIdx

        // When both the signature script and public key script are empty the
        // result is necessarily an error since the stack would end up being
        // empty which is equivalent to a false top element. Thus, just return
        // the relevant error now as an optimization.
        if scripts.iter().all(|e| e.len() == 0) {
            return Err(TxScriptError::FalseStackEntry);
        }

        // TODO: check script is non empty, o.w. skip

        // TODO: isScriptHash(script_pubkey.script)

        // while
        //    Step
        //      validPC
        //      executeOpcode
        //    CheckErrorCondition

        scripts.iter().filter(|s| s.len() > 0).try_for_each(
            |s| self.execute_script(s)
        )
    }

    #[inline]
    fn check_signature(&mut self, hash_type: SigHashType, key: &[u8], sig: &[u8]) -> Result<(), TxScriptError>{
        match self.script_source {
            ScriptSource::TxInput{tx, id, ..} => {
                // TODO: will crash the node. We need to replace it with a proper script engine once it's ready.
                let pk = secp256k1::XOnlyPublicKey::from_slice(key).map_err(|e|TxScriptError::SignatureInvalid(e))?;
                let sig = secp256k1::schnorr::Signature::from_slice(sig).map_err(|e|TxScriptError::SignatureInvalid(e))?;
                let sig_hash = calc_schnorr_signature_hash(tx, id, hash_type, self.reused_values);
                let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
                let sig_cache_key = SigCacheKey { signature: sig, pub_key: pk, message: msg };

                match self.sig_cache.get(&sig_cache_key) {
                    Some(valid) => valid.map_err(|e|TxScriptError::SignatureInvalid(e)),
                    None => {
                        // TODO: Find a way to parallelize this part. This will be less trivial
                        // once this code is inside the script engine.
                        match sig.verify(&msg, &pk) {
                            Ok(()) => {
                                self.sig_cache.insert(sig_cache_key, Ok(()));
                                Ok(())
                            },
                            Err(e) => {
                                self.sig_cache.insert(sig_cache_key, Err(e.clone()));
                                Err(TxScriptError::SignatureInvalid(e))
                            },
                        }
                    }
                }
            },
            _ => Err(TxScriptError::NotATransactionInput)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nop() {
        let mut sig_cache = Cache::new(10_000);
        let mut reused_values = SigHashReusedValues::new();
        let a = vec![0x61u8];
        let mut engine = TxScriptEngine::from_script(a.as_slice(), &reused_values, &sig_cache);
        assert_eq!(engine.execute(), Ok(()));
        assert_eq!(engine.num_ops, 1);

        let a = vec![0x61u8, 0x61u8];
        let mut engine = TxScriptEngine::from_script(a.as_slice(), &reused_values, &sig_cache);
        assert_eq!(engine.execute(), Ok(()));
        assert_eq!(engine.num_ops, 2);
    }
}
