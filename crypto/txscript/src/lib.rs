extern crate core;

mod opcodes;

use core::fmt::{Display, Formatter};
use consensus_core::tx::{PopulatedTransaction, ScriptPublicKey, UtxoEntry};
use crate::opcodes::{OpCodeImplementation, deserialize};
use itertools::Itertools;
use log::warn;



pub const MAX_SCRIPT_PUBLIC_KEY_VERSION: u16 = 0;
pub const MAX_STACK_SIZE: usize = 244;
pub const MAX_OPS_PER_SCRIPT: i32 = 201;

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
    TooManyOperations(i32),
}

impl Display for TxScriptError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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
                Self::TooManyOperations(limit) => format!("exceeded max operation limit of {}", limit)
            }
        )
    }
}

type Stack = Vec<Vec<u8>>;

pub struct TxScriptEngine<'a> {
    dstack: Stack,
    astack: Stack,

    tx: &'a PopulatedTransaction<'a>,
    input_id: usize,
    utxo_entry: &'a UtxoEntry,

    cond_stack: Vec<i8>, // Following if stacks, and whether it is running

    num_ops: i32,
}

impl<'a> TxScriptEngine<'a> {
    pub fn new(tx: &'a PopulatedTransaction<'a>, input_id: usize, utxo_entry: &'a UtxoEntry) -> Self {
        Self{
            dstack: Default::default(),
            astack: Default::default(),
            tx,
            input_id,
            utxo_entry,
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
        if opcode.value() > 16 {
            self.num_ops+=1;
            if self.num_ops > MAX_OPS_PER_SCRIPT {
                return Err(TxScriptError::TooManyOperations(MAX_OPS_PER_SCRIPT));
            }
        }

        // TODO: check if data is not too big
        // TODO: check if in brach?

        // TODO: check minimal data push
        // TODO: run opcode
        Ok(())
    }

    fn execute_script(&mut self, script: &[u8]) -> Result<(), TxScriptError> {

        //let mut consumable = script.iter();
        script.iter().batching(|it| Some(deserialize(it))).try_for_each(|opcode| {
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

    pub fn execute(&mut self, script: &Vec<u8>, script_pubkey: ScriptPublicKey) -> Result<(),TxScriptError> {
        //TODO: removed a check on txIdx

        // When both the signature script and public key script are empty the
        // result is necessarily an error since the stack would end up being
        // empty which is equivalent to a false top element. Thus, just return
        // the relevant error now as an optimization.
        if script.len() == 0 && script_pubkey.script().len() == 0 {
            return Err(TxScriptError::FalseStackEntry);
        }

        if script_pubkey.version() > MAX_SCRIPT_PUBLIC_KEY_VERSION {
            warn!("The version of the scriptPublicKey is higher than the known version - the Execute function returns true.");
            return Ok(());
        }

        // TODO: parseScriptAndVerifySize x2

        let scripts = [script.clone(), script_pubkey.script().to_vec()];
        let unified_script = script.clone();
        // TODO: check script is non empty, o.w. skip

        // TODO: isScriptHash(script_pubkey.script)

        // while
        //    Step
        //      validPC
        //      executeOpcode
        //    CheckErrorCondition

        let _ = scripts.iter().map(|x| self.execute_script(x));
        Ok(())
    }

    pub fn clean_execute(tx: &'a PopulatedTransaction, input_id: usize, utxo_entry: &'a UtxoEntry) -> Result<(), TxScriptError> {
        let mut engine = TxScriptEngine::new(tx, input_id, utxo_entry);
        let script = &tx.tx.inputs[input_id].signature_script;
        //script: Vec<u8>, script_pubkey: ScriptPublicKey
        let script_pubkey = ScriptPublicKey::default();
        //&tx.inputs[input_id].previous_out

        engine.execute(script, script_pubkey)?;
        Ok(())
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bad_pc() {

    }
}
