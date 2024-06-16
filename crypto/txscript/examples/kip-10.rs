//! # Kaspa Transaction Script Example
//!
//! This example demonstrates the use of custom opcodes and script execution within the Kaspa blockchain ecosystem.
//! There are two main scenarios:
//!
//! 1. **Owner scenario:** The script checks if the input is used by the owner and verifies the owner's signature.
//! 2. **Borrower scenario:** The script allows the input to be consumed if the output with the same index has a value of input + threshold and goes to the P2SH of the script itself. This scenario also includes a check where the threshold is not reached.

use kaspa_consensus_core::{
    hashing::sighash::{calc_schnorr_signature_hash, SigHashReusedValues},
    hashing::sighash_type::SIG_HASH_ALL,
    tx::{
        MutableTransaction, PopulatedTransaction, Transaction, TransactionId, TransactionInput, TransactionOutpoint,
        TransactionOutput, UtxoEntry, VerifiableTransaction,
    },
};
use kaspa_txscript::{
    caches::Cache,
    opcodes::codes::{
        OpCheckSig, OpDup, OpElse, OpEndIf, OpEqualVerify, OpFalse, OpGreaterThanOrEqual, OpIf, OpInputAmount, OpInputSpk,
        OpOutputAmount, OpOutputSpk, OpSub, OpTrue,
    },
    pay_to_script_hash_script,
    script_builder::{ScriptBuilder, ScriptBuilderResult},
    TxScriptEngine,
};
use kaspa_txscript_errors::TxScriptError::EvalFalse;
use rand::thread_rng;
use secp256k1::Keypair;

/// Main function to execute the Kaspa transaction script example.
///
/// # Returns
///
/// * `ScriptBuilderResult<()>` - Result of script builder operations.
fn main() -> ScriptBuilderResult<()> {
    // Create a new key pair for the owner
    let owner = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());

    // Set a threshold value for comparison
    let threshold: i64 = 100;

    // Initialize a cache for signature verification
    let sig_cache = Cache::new(10_000);

    // Prepare to reuse values for signature hashing
    let mut reused_values = SigHashReusedValues::new();

    // Create the script builder
    let mut builder = ScriptBuilder::new();
    let script = builder
        // Owner branch
        .add_op(OpIf)?
        .add_op(OpDup)?
        .add_data(owner.x_only_public_key().0.serialize().as_slice())?
        .add_op(OpEqualVerify)?
        .add_op(OpCheckSig)?
        // Borrower branch
        .add_op(OpElse)?
        .add_ops(&[OpInputSpk, OpOutputSpk, OpEqualVerify, OpOutputAmount])?
        .add_i64(threshold)?
        .add_ops(&[OpSub, OpInputAmount, OpGreaterThanOrEqual])?
        .add_op(OpEndIf)?
        .drain();

    // Generate the script public key
    let spk = pay_to_script_hash_script(&script);

    // Define the input value
    let input_value = 1000000000;

    // Create a transaction output
    let output = TransactionOutput { value: 1000000000 + threshold as u64, script_public_key: spk.clone() };

    // Create a UTXO entry for the input
    let utxo_entry = UtxoEntry::new(input_value, spk, 0, false);

    // Create a transaction input
    let input = TransactionInput {
        previous_outpoint: TransactionOutpoint {
            transaction_id: TransactionId::from_bytes([
                0xc9, 0x97, 0xa5, 0xe5, 0x6e, 0x10, 0x42, 0x02, 0xfa, 0x20, 0x9c, 0x6a, 0x85, 0x2d, 0xd9, 0x06, 0x60, 0xa2, 0x0b,
                0x2d, 0x9c, 0x35, 0x24, 0x23, 0xed, 0xce, 0x25, 0x85, 0x7f, 0xcd, 0x37, 0x04,
            ]),
            index: 0,
        },
        signature_script: ScriptBuilder::new().add_data(&script)?.drain(),
        sequence: 4294967295,
        sig_op_count: 0,
    };

    // Create a transaction with the input and output
    let mut tx = Transaction::new(1, vec![input.clone()], vec![output.clone()], 0, Default::default(), 0, vec![]);

    // Check owner branch
    {
        println!("check owner scenario");
        let mut tx = MutableTransaction::with_entries(tx.clone(), vec![utxo_entry.clone()]);
        let sig_hash = calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &mut reused_values);
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();

        let sig = owner.sign_schnorr(msg);
        let mut signature = Vec::new();
        signature.extend_from_slice(sig.as_ref().as_slice());
        signature.push(SIG_HASH_ALL.to_u8());

        let mut builder = ScriptBuilder::new();
        builder.add_data(&signature)?;
        builder.add_data(owner.x_only_public_key().0.serialize().as_slice())?;
        builder.add_op(OpTrue)?;
        builder.add_data(&script)?;
        {
            tx.tx.inputs[0].signature_script = builder.drain();
        }

        let tx = tx.as_verifiable();
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.inputs()[0], 0, &utxo_entry, &mut reused_values, &sig_cache, true)
                .expect("Script creation failed");
        assert_eq!(vm.execute(), Ok(()));
        println!("owner scenario successes");
    }

    // Check borrower branch
    {
        println!("check borrower scenario");
        tx.inputs[0].signature_script = ScriptBuilder::new().add_op(OpFalse)?.add_data(&script)?.drain();
        let tx = PopulatedTransaction::new(&tx, vec![utxo_entry.clone()]);
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.tx.inputs[0], 0, &utxo_entry, &mut reused_values, &sig_cache, true)
                .expect("Script creation failed");
        assert_eq!(vm.execute(), Ok(()));
        println!("borrower scenario successes");
    }

    // Check borrower branch with threshold not reached
    {
        println!("check borrower scenario with underflow");
        // Less than threshold
        tx.outputs[0].value -= 1;
        let tx = PopulatedTransaction::new(&tx, vec![utxo_entry.clone()]);
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.tx.inputs[0], 0, &utxo_entry, &mut reused_values, &sig_cache, true)
                .expect("Script creation failed");
        assert_eq!(vm.execute(), Err(EvalFalse));
        println!("borrower scenario with underflow failed! all good");
    }

    Ok(())
}
