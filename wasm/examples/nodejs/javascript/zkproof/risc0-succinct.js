const { PrivateKey, RpcClient, ScriptBuilder, Opcodes,
    payToScriptHashScript, addressFromScriptPublicKey,
    createTransaction, signTransaction } = require('../../../../nodejs/kaspa');
const fs = require('fs');
const path = require('path');

// Configuration
const NETWORK_ID = 'testnet-12';
const RPC_URL = 'ws://127.0.0.1:16310';
const PRIVATE_KEY = 'b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f';

// ZK Proof configuration
const ZK_VERIFIER_TAG = 0x21; // R0Succinct

// Data files - matches the stack layout expected by R0SuccinctPrecompile::verify_zk
// Stack (bottom to top): seal, claim, hashfn, control_index, control_digests, journal, image_id, tag
const SEAL_FILE = './succinct.seal.hex';
const CLAIM_FILE = './succinct.claim.hex';
const HASHFN_FILE = './succinct.hashfn.hex';
const CONTROL_INDEX_FILE = './succinct.control_index.hex';
const CONTROL_DIGESTS_FILE = './succinct.control_digests.hex';
const JOURNAL_FILE = './succinct.journal.hex';
const IMAGE_ID_FILE = './succinct.image.hex';

function loadHexFile(filePath, label) {
    const hex = fs.readFileSync(filePath, 'utf8').trim();
    const cleanHex = hex.startsWith('0x') ? hex.slice(2) : hex;
    const buf = Buffer.from(cleanHex, 'hex');
    console.log(`Loaded ${label}: ${buf.length} bytes`);
    return buf;
}

async function zktest() {
    const privateKey = new PrivateKey(PRIVATE_KEY);
    const keypair = privateKey.toKeypair();
    const sourceAddress = keypair.toAddress(NETWORK_ID);
    console.log(`Using source address: ${sourceAddress}`);

    // Load all proof components
    let seal, claim, hashfn, controlIndex, controlDigests, journal, imageId;
    try {
        seal = loadHexFile(SEAL_FILE, 'seal');
        claim = loadHexFile(CLAIM_FILE, 'claim');
        hashfn = loadHexFile(HASHFN_FILE, 'hashfn');
        controlIndex = loadHexFile(CONTROL_INDEX_FILE, 'control_index');
        controlDigests = loadHexFile(CONTROL_DIGESTS_FILE, 'control_digests');
        journal = loadHexFile(JOURNAL_FILE, 'journal');
        imageId = loadHexFile(IMAGE_ID_FILE, 'image_id');
    } catch (error) {
        console.error('Failed to load proof data:', error.message);
        return;
    }

    const rpc = new RpcClient({
        url: RPC_URL,
        encoding: 'borsh',
        networkId: NETWORK_ID
    });

    await rpc.connect();

    try {
        // Get UTXOs and wait for maturity
        console.log('Fetching UTXOs...');
        let response = await rpc.getUtxosByAddresses([sourceAddress]);

        const info = await rpc.getBlockDagInfo();
        const currentDaaScore = info.virtualDaaScore;

        const matureUtxos = response.entries.filter(entry => {
            if (!entry.entry.isCoinbase) return true;
            return (currentDaaScore - entry.entry.blockDaaScore) >= 100n;
        });

        if (matureUtxos.length === 0) {
            console.log('No mature UTXOs available. Mining or waiting required.');
            return;
        }

        console.log(`Found ${matureUtxos.length} mature UTXOs`);

        // Create P2SH redeem script
        // The redeem script pushes journal, image_id, and tag, then calls OpZkPrecompile.
        // The signature script provides: seal, claim, hashfn, control_index, control_digests.
        //
        // Combined stack (bottom to top) before OpZkPrecompile:
        //   seal, claim, hashfn, control_index, control_digests, journal, image_id, tag
        const redeemScript = new ScriptBuilder()
            .addData(journal)                              // Push journal onto stack
            .addData(imageId)                              // Push image ID onto stack
            .addData(Buffer.from([ZK_VERIFIER_TAG]))       // Push tag (0x21) onto stack
            .addOp(Opcodes.OpZkPrecompile)                 // Execute ZK verification
            .drain();

        const lockingScript = payToScriptHashScript(redeemScript);
        console.log(`Locking script created:`, lockingScript);
        console.log(`Redeem script (hex): ${redeemScript}`);

        // Convert the P2SH script to an address
        const p2shAddress = addressFromScriptPublicKey(lockingScript, NETWORK_ID);
        console.log(`P2SH address: ${p2shAddress}`);

        // Create COMMIT transaction
        const utxoToSpend = matureUtxos[0];
        const commitAmount = utxoToSpend.amount - 10000n;

        // Convert UTXO to IUtxoEntry format
        const utxoEntries = [{
            address: sourceAddress,
            outpoint: {
                transactionId: utxoToSpend.outpoint.transactionId,
                index: utxoToSpend.outpoint.index
            },
            scriptPublicKey: utxoToSpend.entry.scriptPublicKey,
            amount: utxoToSpend.amount,
            isCoinbase: utxoToSpend.entry.isCoinbase,
            blockDaaScore: utxoToSpend.entry.blockDaaScore
        }];

        // Create commit transaction sending to P2SH address
        const commitTx = createTransaction(
            utxoEntries,
            [{
                address: p2shAddress,
                amount: commitAmount
            }],
            0n,
            '',
            1
        );

        console.log('Commit transaction created');

        // Sign the commit transaction
        const signedCommitTx = signTransaction(commitTx, [privateKey], false);
        console.log('Commit transaction signed');

        // Submit commit transaction
        const submitResult = await rpc.submitTransaction({ transaction: signedCommitTx });

        // Extract the transaction ID from the result
        const commitTxId = submitResult.transactionId || submitResult;
        console.log(`Commit transaction submitted: ${commitTxId}`);

        // Wait for commit transaction confirmation
        console.log('Waiting for commit transaction to be accepted...');
        await new Promise(resolve => setTimeout(resolve, 5000));

        // Create REDEEM transaction
        console.log('Creating redeem transaction...');

        // Build signature script for P2SH spending.
        // Push proof components that are NOT in the redeem script, then the redeem script itself.
        // These go onto the stack before the redeem script executes.
        const signatureScript = new ScriptBuilder()
            .addData(seal)                                   // Push seal (bottom of proof stack)
            .addData(claim)                                  // Push claim
            .addData(hashfn)                                 // Push hash function id
            .addData(controlIndex)                           // Push control inclusion proof index
            .addData(controlDigests)                         // Push control inclusion proof digests
            .addData(Buffer.from(redeemScript, 'hex'))       // Push redeem script (P2SH requirement)
            .drain();

        console.log(`Signature script length: ${Buffer.from(signatureScript, 'hex').length} bytes`);

        // Construct the P2SH UTXO entry
        const p2shUtxoEntry = {
            address: p2shAddress,
            outpoint: {
                transactionId: commitTxId,
                index: 0
            },
            scriptPublicKey: lockingScript,
            amount: commitAmount,
            isCoinbase: false,
            blockDaaScore: currentDaaScore
        };

        // Create redeem transaction
        const redeemTx = createTransaction(
            [p2shUtxoEntry],
            [{
                address: sourceAddress,
                amount: commitAmount - 1000000n
            }],
            0n,
            '',
            250
        );

        console.log(redeemTx)

        // Set the signature script
        redeemTx.inputs[0].signatureScript = signatureScript;

        console.log('Redeem transaction created');
        console.log('Redeem script contains:');
        console.log('  - Journal:', journal.toString('hex'));
        console.log('  - Image ID:', imageId.toString('hex'));
        console.log('  - Tag: 0x21 (R0Succinct)');
        console.log('  - OpZkPrecompile');
        console.log('Signature script provides: seal, claim, hashfn, control_index, control_digests');

        // Submit redeem transaction
        const redeemResult = await rpc.submitTransaction({ transaction: redeemTx });
        const redeemTxId = redeemResult.transactionId || redeemResult;
        console.log(`Redeem transaction submitted: ${redeemTxId}`);
        console.log('ZK proof verification successful!');

    } catch (error) {
        console.error('Error:', error);
        if (error.stack) {
            console.error('Stack:', error.stack);
        }
    } finally {
        await rpc.disconnect();
    }
}

zktest().catch(console.error);
