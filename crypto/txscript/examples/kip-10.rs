use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::{
    hashing::{
        sighash::{calc_schnorr_signature_hash, SigHashReusedValuesUnsync},
        sighash_type::SIG_HASH_ALL,
    },
    tx::{
        MutableTransaction, PopulatedTransaction, Transaction, TransactionId, TransactionInput, TransactionOutpoint,
        TransactionOutput, UtxoEntry, VerifiableTransaction,
    },
};
use kaspa_txscript::{
    caches::Cache,
    opcodes::codes::{
        OpCheckSig, OpCheckSigVerify, OpDup, OpElse, OpEndIf, OpEqualVerify, OpFalse, OpGreaterThanOrEqual, OpIf, OpSub, OpTrue,
        OpTxInputAmount, OpTxInputIndex, OpTxInputSpk, OpTxOutputAmount, OpTxOutputSpk,
    },
    pay_to_address_script, pay_to_script_hash_script,
    script_builder::{ScriptBuilder, ScriptBuilderResult},
    TxScriptEngine,
};
use kaspa_txscript_errors::TxScriptError::{EvalFalse, VerifyError};
use rand::thread_rng;
use secp256k1::Keypair;

/// Main function to execute all Kaspa transaction script scenarios.
///
/// # Returns
///
/// * `ScriptBuilderResult<()>` - Result of script builder operations for all scenarios.
fn main() -> ScriptBuilderResult<()> {
    threshold_scenario()?;
    threshold_scenario_limited_one_time()?;
    threshold_scenario_limited_2_times()?;
    shared_secret_scenario()?;
    Ok(())
}

/// # Standard Threshold Scenario
///
/// This scenario demonstrates the use of custom opcodes and script execution within the Kaspa blockchain ecosystem.
/// There are two main cases:
///
/// 1. **Owner case:** The script checks if the input is used by the owner and verifies the owner's signature.
/// 2. **Borrower case:** The script allows the input to be consumed if the output with the same index has a value of input + threshold and goes to the P2SH of the script itself.
///
/// # Returns
///
/// * `ScriptBuilderResult<()>` - Result of script builder operations for this scenario.
fn threshold_scenario() -> ScriptBuilderResult<()> {
    println!("\n[STANDARD] Running standard threshold scenario");
    // Create a new key pair for the owner
    let owner = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());

    // Set a threshold value for comparison
    let threshold: i64 = 100;

    // Initialize a cache for signature verification
    let sig_cache = Cache::new(10_000);

    // Prepare to reuse values for signature hashing
    let reused_values = SigHashReusedValuesUnsync::new();

    // Create the script builder
    let mut builder = ScriptBuilder::new();
    let script = builder
        // Owner branch
        .add_op(OpIf)?
        .add_data(owner.x_only_public_key().0.serialize().as_slice())?
        .add_op(OpCheckSig)?
        // Borrower branch
        .add_op(OpElse)?
        .add_ops(&[OpTxInputIndex, OpTxInputSpk, OpTxInputIndex, OpTxOutputSpk, OpEqualVerify, OpTxInputIndex, OpTxOutputAmount])?
        .add_i64(threshold)?
        .add_ops(&[OpSub, OpTxInputIndex, OpTxInputAmount, OpGreaterThanOrEqual])?
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
        sig_op_count: 1,
    };

    // Create a transaction with the input and output
    let mut tx = Transaction::new(1, vec![input.clone()], vec![output.clone()], 0, Default::default(), 0, vec![]);

    // Check owner branch
    {
        println!("[STANDARD] Checking owner branch");
        let mut tx = MutableTransaction::with_entries(tx.clone(), vec![utxo_entry.clone()]);
        let sig_hash = calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &reused_values);
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();

        let sig = owner.sign_schnorr(msg);
        let mut signature = Vec::new();
        signature.extend_from_slice(sig.as_ref().as_slice());
        signature.push(SIG_HASH_ALL.to_u8());

        let mut builder = ScriptBuilder::new();
        builder.add_data(&signature)?;
        builder.add_op(OpTrue)?;
        builder.add_data(&script)?;
        {
            tx.tx.inputs[0].signature_script = builder.drain();
        }

        let tx = tx.as_verifiable();
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.inputs()[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Ok(()));
        println!("[STANDARD] Owner branch execution successful");
    }

    // Check borrower branch
    {
        println!("[STANDARD] Checking borrower branch");
        tx.inputs[0].signature_script = ScriptBuilder::new().add_op(OpFalse)?.add_data(&script)?.drain();
        let tx = PopulatedTransaction::new(&tx, vec![utxo_entry.clone()]);
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.tx.inputs[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Ok(()));
        println!("[STANDARD] Borrower branch execution successful");
    }

    // Check borrower branch with threshold not reached
    {
        println!("[STANDARD] Checking borrower branch with threshold not reached");
        // Less than threshold
        tx.outputs[0].value -= 1;
        let tx = PopulatedTransaction::new(&tx, vec![utxo_entry.clone()]);
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.tx.inputs[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Err(EvalFalse));
        println!("[STANDARD] Borrower branch with threshold not reached failed as expected");
    }

    println!("[STANDARD] Standard threshold scenario completed successfully");
    Ok(())
}

/// Generate a script for limited-time borrowing scenarios
///
/// This function creates a script that allows for limited-time borrowing with a threshold,
/// or spending by the owner at any time. It's generic enough to be used for both one-time
/// and multi-time borrowing scenarios.
///
/// # Arguments
///
/// * `owner` - The public key of the owner
/// * `threshold` - The threshold amount that must be met for borrowing
/// * `output_spk` - The output script public key as a vector of bytes
///
/// # Returns
///
/// * The generated script as a vector of bytes
fn generate_limited_time_script(owner: &Keypair, threshold: i64, output_spk: Vec<u8>) -> ScriptBuilderResult<Vec<u8>> {
    let mut builder = ScriptBuilder::new();
    let script = builder
        // Owner branch
        .add_op(OpIf)?
        .add_data(owner.x_only_public_key().0.serialize().as_slice())?
        .add_op(OpCheckSig)?
        // Borrower branch
        .add_op(OpElse)?
        .add_data(&output_spk)?
        .add_ops(&[OpTxInputIndex, OpTxOutputSpk, OpEqualVerify, OpTxInputIndex, OpTxOutputAmount])?
        .add_i64(threshold)?
        .add_ops(&[OpSub, OpTxInputIndex, OpTxInputAmount, OpGreaterThanOrEqual])?
        .add_op(OpEndIf)?
        .drain();

    Ok(script)
}

// Helper function to create P2PK script as a vector
fn p2pk_as_vec(owner: &Keypair) -> Vec<u8> {
    let p2pk =
        pay_to_address_script(&Address::new(Prefix::Mainnet, Version::PubKey, owner.x_only_public_key().0.serialize().as_slice()));
    let version = p2pk.version.to_be_bytes();
    let script = p2pk.script();
    let mut v = Vec::with_capacity(version.len() + script.len());
    v.extend_from_slice(&version);
    v.extend_from_slice(script);
    v
}

/// # Threshold Scenario with Limited One-Time Borrowing
///
/// This function demonstrates a modified version of the threshold scenario where borrowing
/// is limited to a single occurrence. The key difference from the standard threshold scenario
/// is that the output goes to a Pay-to-Public-Key (P2PK) address instead of a Pay-to-Script-Hash (P2SH)
/// address of the script itself.
///
/// ## Key Features:
/// 1. **One-Time Borrowing:** The borrower can only use this mechanism once, as the funds are
///    sent to a regular P2PK address instead of back to the script.
/// 2. **Owner Access:** The owner retains the ability to spend the funds at any time using their private key.
/// 3. **Threshold Mechanism:** The borrower must still meet the threshold requirement to spend the funds.
/// 4. **Output Validation:** Ensures the output goes to the correct address.
///
/// ## Scenarios Tested:
/// 1. **Owner Spending:** Verifies that the owner can spend the funds using their signature.
/// 2. **Borrower Spending:** Checks if the borrower can spend when meeting the threshold and
///    sending to the correct P2PK address.
/// 3. **Invalid Borrower Attempt (Threshold):** Ensures the script fails if the borrower doesn't meet the threshold.
/// 4. **Invalid Borrower Attempt (Wrong Output):** Ensures the script fails if the output goes to an incorrect address.
///
/// # Returns
///
/// * `ScriptBuilderResult<()>` - Result of script builder operations for this scenario.
fn threshold_scenario_limited_one_time() -> ScriptBuilderResult<()> {
    println!("\n[ONE-TIME] Running threshold one-time scenario");
    // Create a new key pair for the owner
    let owner = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());

    // Set a threshold value for comparison
    let threshold: i64 = 100;

    let p2pk =
        pay_to_address_script(&Address::new(Prefix::Mainnet, Version::PubKey, owner.x_only_public_key().0.serialize().as_slice()));
    let p2pk_vec = p2pk_as_vec(&owner);
    let script = generate_limited_time_script(&owner, threshold, p2pk_vec.clone())?;

    // Initialize a cache for signature verification
    let sig_cache = Cache::new(10_000);

    // Prepare to reuse values for signature hashing
    let reused_values = SigHashReusedValuesUnsync::new();

    // Generate the script public key
    let spk = pay_to_script_hash_script(&script);

    // Define the input value
    let input_value = 1000000000;

    // Create a transaction output
    let output = TransactionOutput { value: 1000000000 + threshold as u64, script_public_key: p2pk.clone() };

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
        sig_op_count: 1,
    };

    // Create a transaction with the input and output
    let mut tx = Transaction::new(1, vec![input.clone()], vec![output.clone()], 0, Default::default(), 0, vec![]);

    // Check owner branch
    {
        println!("[ONE-TIME] Checking owner branch");
        let mut tx = MutableTransaction::with_entries(tx.clone(), vec![utxo_entry.clone()]);
        let sig_hash = calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &reused_values);
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();

        let sig = owner.sign_schnorr(msg);
        let mut signature = Vec::new();
        signature.extend_from_slice(sig.as_ref().as_slice());
        signature.push(SIG_HASH_ALL.to_u8());

        let mut builder = ScriptBuilder::new();
        builder.add_data(&signature)?;
        builder.add_op(OpTrue)?;
        builder.add_data(&script)?;
        {
            tx.tx.inputs[0].signature_script = builder.drain();
        }

        let tx = tx.as_verifiable();
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.inputs()[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Ok(()));
        println!("[ONE-TIME] Owner branch execution successful");
    }

    // Check borrower branch
    {
        println!("[ONE-TIME] Checking borrower branch");
        tx.inputs[0].signature_script = ScriptBuilder::new().add_op(OpFalse)?.add_data(&script)?.drain();
        let tx = PopulatedTransaction::new(&tx, vec![utxo_entry.clone()]);
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.tx.inputs[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Ok(()));
        println!("[ONE-TIME] Borrower branch execution successful");
    }

    // Check borrower branch with threshold not reached
    {
        println!("[ONE-TIME] Checking borrower branch with threshold not reached");
        // Less than threshold
        tx.outputs[0].value -= 1;
        let tx = PopulatedTransaction::new(&tx, vec![utxo_entry.clone()]);
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.tx.inputs[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Err(EvalFalse));
        println!("[ONE-TIME] Borrower branch with threshold not reached failed as expected");
    }

    // Check borrower branch with output going to wrong address
    {
        println!("[ONE-TIME] Checking borrower branch with output going to wrong address");
        // Create a new key pair for a different address
        let wrong_recipient = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());
        let wrong_p2pk = pay_to_address_script(&Address::new(
            Prefix::Mainnet,
            Version::PubKey,
            wrong_recipient.x_only_public_key().0.serialize().as_slice(),
        ));

        // Create a new transaction with the wrong output address
        let mut wrong_tx = tx.clone();
        wrong_tx.outputs[0].script_public_key = wrong_p2pk;
        wrong_tx.inputs[0].signature_script = ScriptBuilder::new().add_op(OpFalse)?.add_data(&script)?.drain();

        let wrong_tx = PopulatedTransaction::new(&wrong_tx, vec![utxo_entry.clone()]);
        let mut vm = TxScriptEngine::from_transaction_input(
            &wrong_tx,
            &wrong_tx.tx.inputs[0],
            0,
            &utxo_entry,
            &reused_values,
            &sig_cache,
            true,
            false,
        );
        assert_eq!(vm.execute(), Err(VerifyError));
        println!("[ONE-TIME] Borrower branch with output going to wrong address failed as expected");
    }

    println!("[ONE-TIME] Threshold one-time scenario completed successfully");
    Ok(())
}

/// # Threshold Scenario with Limited Two-Times Borrowing
///
/// This function demonstrates a modified version of the threshold scenario where borrowing
/// is limited to two occurrences. The key difference from the one-time scenario is that
/// the first borrowing outputs to a P2SH of the one-time script, allowing for a second borrowing.
///
/// ## Key Features:
/// 1. **Two-Times Borrowing:** The borrower can use this mechanism twice.
/// 2. **Owner Access:** The owner retains the ability to spend the funds at any time using their private key.
/// 3. **Threshold Mechanism:** The borrower must still meet the threshold requirement to spend the funds.
/// 4. **Output Validation:** Ensures the output goes to the correct address (P2SH of one-time script for first borrow).
///
/// ## Scenarios Tested:
/// 1. **Owner Spending:** Verifies that the owner can spend the funds using their signature.
/// 2. **Borrower First Spending:** Checks if the borrower can spend when meeting the threshold and
///    sending to the correct P2SH address of the one-time script.
/// 3. **Invalid Borrower Attempt (Threshold):** Ensures the script fails if the borrower doesn't meet the threshold.
/// 4. **Invalid Borrower Attempt (Wrong Output):** Ensures the script fails if the output goes to an incorrect address.
///
/// # Returns
///
/// * `ScriptBuilderResult<()>` - Result of script builder operations for this scenario.
fn threshold_scenario_limited_2_times() -> ScriptBuilderResult<()> {
    println!("\n[TWO-TIMES] Running threshold two-times scenario");
    let owner = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());
    let threshold: i64 = 100;

    // First, create the one-time script
    let p2pk_vec = p2pk_as_vec(&owner);
    let one_time_script = generate_limited_time_script(&owner, threshold, p2pk_vec)?;

    // Now, create the two-times script using the one-time script as output
    let p2sh_one_time = pay_to_script_hash_script(&one_time_script);
    let p2sh_one_time_vec = {
        let version = p2sh_one_time.version.to_be_bytes();
        let script = p2sh_one_time.script();
        let mut v = Vec::with_capacity(version.len() + script.len());
        v.extend_from_slice(&version);
        v.extend_from_slice(script);
        v
    };

    let two_times_script = generate_limited_time_script(&owner, threshold, p2sh_one_time_vec)?;

    // Initialize a cache for signature verification
    let sig_cache = Cache::new(10_000);

    // Prepare to reuse values for signature hashing
    let reused_values = SigHashReusedValuesUnsync::new();

    // Generate the script public key
    let spk = pay_to_script_hash_script(&two_times_script);

    // Define the input value
    let input_value = 1000000000;

    // Create a transaction output
    let output = TransactionOutput { value: 1000000000 + threshold as u64, script_public_key: p2sh_one_time };

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
        signature_script: ScriptBuilder::new().add_data(&two_times_script)?.drain(),
        sequence: 4294967295,
        sig_op_count: 1,
    };

    // Create a transaction with the input and output
    let mut tx = Transaction::new(1, vec![input.clone()], vec![output.clone()], 0, Default::default(), 0, vec![]);

    // Check owner branch
    {
        println!("[TWO-TIMES] Checking owner branch");
        let mut tx = MutableTransaction::with_entries(tx.clone(), vec![utxo_entry.clone()]);
        let sig_hash = calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &reused_values);
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();

        let sig = owner.sign_schnorr(msg);
        let mut signature = Vec::new();
        signature.extend_from_slice(sig.as_ref().as_slice());
        signature.push(SIG_HASH_ALL.to_u8());

        let mut builder = ScriptBuilder::new();
        builder.add_data(&signature)?;
        builder.add_op(OpTrue)?;
        builder.add_data(&two_times_script)?;
        {
            tx.tx.inputs[0].signature_script = builder.drain();
        }

        let tx = tx.as_verifiable();
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.inputs()[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Ok(()));
        println!("[TWO-TIMES] Owner branch execution successful");
    }

    // Check borrower branch (first borrowing)
    {
        println!("[TWO-TIMES] Checking borrower branch (first borrowing)");
        tx.inputs[0].signature_script = ScriptBuilder::new().add_op(OpFalse)?.add_data(&two_times_script)?.drain();
        let tx = PopulatedTransaction::new(&tx, vec![utxo_entry.clone()]);
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.tx.inputs[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Ok(()));
        println!("[TWO-TIMES] Borrower branch (first borrowing) execution successful");
    }

    // Check borrower branch with threshold not reached
    {
        println!("[TWO-TIMES] Checking borrower branch with threshold not reached");
        // Less than threshold
        tx.outputs[0].value -= 1;
        let tx = PopulatedTransaction::new(&tx, vec![utxo_entry.clone()]);
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.tx.inputs[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Err(EvalFalse));
        println!("[TWO-TIMES] Borrower branch with threshold not reached failed as expected");
    }

    // Check borrower branch with output going to wrong address
    {
        println!("[TWO-TIMES] Checking borrower branch with output going to wrong address");
        // Create a new key pair for a different address
        let wrong_recipient = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());
        let wrong_p2pk = pay_to_address_script(&Address::new(
            Prefix::Mainnet,
            Version::PubKey,
            wrong_recipient.x_only_public_key().0.serialize().as_slice(),
        ));

        // Create a new transaction with the wrong output address
        let mut wrong_tx = tx.clone();
        wrong_tx.outputs[0].script_public_key = wrong_p2pk;
        wrong_tx.inputs[0].signature_script = ScriptBuilder::new().add_op(OpFalse)?.add_data(&two_times_script)?.drain();

        let wrong_tx = PopulatedTransaction::new(&wrong_tx, vec![utxo_entry.clone()]);
        let mut vm = TxScriptEngine::from_transaction_input(
            &wrong_tx,
            &wrong_tx.tx.inputs[0],
            0,
            &utxo_entry,
            &reused_values,
            &sig_cache,
            true,
            false,
        );
        assert_eq!(vm.execute(), Err(VerifyError));
        println!("[TWO-TIMES] Borrower branch with output going to wrong address failed as expected");
    }

    println!("[TWO-TIMES] Threshold two-times scenario completed successfully");
    Ok(())
}

/// # Shared Secret Scenario
///
/// This scenario demonstrates the use of a shared secret within the Kaspa blockchain ecosystem.
/// Instead of using a threshold value, it checks the shared secret and the signature associated with it.
///
/// ## Key Features:
/// 1. **Owner Access:** The owner can spend funds at any time using their signature.
/// 2. **Shared Secret:** A separate keypair is used as a shared secret for borrower access.
/// 3. **Borrower Verification:** The borrower must provide the correct shared secret signature to spend.
///
/// ## Scenarios Tested:
/// 1. **Owner Spending:** Verifies that the owner can spend the funds using their signature.
/// 2. **Borrower with Correct Secret:** Checks if the borrower can spend when providing the correct shared secret.
/// 3. **Borrower with Incorrect Secret:** Ensures the script fails if the borrower uses an incorrect secret.
///
/// # Returns
///
/// * `ScriptBuilderResult<()>` - Result of script builder operations for this scenario.
fn shared_secret_scenario() -> ScriptBuilderResult<()> {
    println!("\n[SHARED-SECRET] Running shared secret scenario");

    // Create key pairs for the owner, shared secret, and a potential borrower
    let owner = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());
    let shared_secret_kp = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());
    let borrower_kp = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());

    // Initialize a cache for signature verification
    let sig_cache = Cache::new(10_000);

    // Create the script builder
    let mut builder = ScriptBuilder::new();
    let script = builder
        // Owner branch
        .add_op(OpIf)?
        .add_data(owner.x_only_public_key().0.serialize().as_slice())?
        .add_op(OpCheckSig)?
        // Borrower branch
        .add_op(OpElse)?
        .add_op(OpDup)?
        .add_data(shared_secret_kp.x_only_public_key().0.serialize().as_slice())?
        .add_op(OpEqualVerify)?
        .add_op(OpCheckSigVerify)?
        .add_ops(&[OpTxInputIndex, OpTxInputSpk, OpTxInputIndex, OpTxOutputSpk, OpEqualVerify, OpTxInputIndex, OpTxOutputAmount, OpTxInputIndex, OpTxInputAmount, OpGreaterThanOrEqual])?
        .add_op(OpEndIf)?
        .drain();

    // Generate the script public key
    let spk = pay_to_script_hash_script(&script);

    // Define the input value
    let input_value = 1000000000;

    // Create a transaction output
    let output = TransactionOutput { value: input_value, script_public_key: spk.clone() };

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
        sig_op_count: 1,
    };

    // Create a transaction with the input and output
    let tx = Transaction::new(1, vec![input.clone()], vec![output.clone()], 0, Default::default(), 0, vec![]);
    let sign = |pk: Keypair| {
        // Prepare to reuse values for signature hashing
        let reused_values = SigHashReusedValuesUnsync::new();

        let tx = MutableTransaction::with_entries(tx.clone(), vec![utxo_entry.clone()]);
        let sig_hash = calc_schnorr_signature_hash(&tx.as_verifiable(), 0, SIG_HASH_ALL, &reused_values);
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();

        let sig = pk.sign_schnorr(msg);
        let mut signature = Vec::new();
        signature.extend_from_slice(sig.as_ref().as_slice());
        signature.push(SIG_HASH_ALL.to_u8());
        (tx, signature, reused_values)
    };

    // Check owner branch
    {
        println!("[SHARED-SECRET] Checking owner branch");
        let (mut tx, signature, reused_values) = sign(owner);
        let mut builder = ScriptBuilder::new();
        builder.add_data(&signature)?;
        builder.add_op(OpTrue)?;
        builder.add_data(&script)?;
        {
            tx.tx.inputs[0].signature_script = builder.drain();
        }

        let tx = tx.as_verifiable();
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.inputs()[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Ok(()));
        println!("[SHARED-SECRET] Owner branch execution successful");
    }

    // Check borrower branch with correct shared secret
    {
        println!("[SHARED-SECRET] Checking borrower branch with correct shared secret");
        let (mut tx, signature, reused_values) = sign(shared_secret_kp);
        builder.add_data(&signature)?;
        builder.add_data(shared_secret_kp.x_only_public_key().0.serialize().as_slice())?;
        builder.add_op(OpFalse)?;
        builder.add_data(&script)?;
        {
            tx.tx.inputs[0].signature_script = builder.drain();
        }

        let tx = tx.as_verifiable();
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.inputs()[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Ok(()));
        println!("[SHARED-SECRET] Borrower branch with correct shared secret execution successful");
    }

    // Check borrower branch with incorrect secret
    {
        println!("[SHARED-SECRET] Checking borrower branch with incorrect secret");
        let (mut tx, signature, reused_values) = sign(borrower_kp);
        builder.add_data(&signature)?;
        builder.add_data(borrower_kp.x_only_public_key().0.serialize().as_slice())?;
        builder.add_op(OpFalse)?;
        builder.add_data(&script)?;
        {
            tx.tx.inputs[0].signature_script = builder.drain();
        }

        let tx = tx.as_verifiable();
        let mut vm =
            TxScriptEngine::from_transaction_input(&tx, &tx.inputs()[0], 0, &utxo_entry, &reused_values, &sig_cache, true, false);
        assert_eq!(vm.execute(), Err(VerifyError));
        println!("[SHARED-SECRET] Borrower branch with incorrect secret failed as expected");
    }

    println!("[SHARED-SECRET] Shared secret scenario completed successfully");
    Ok(())
}
