const { PrivateKey, RpcClient, ScriptBuilder, Opcodes,
    payToScriptHashScript, addressFromScriptPublicKey,
    createTransaction, signTransaction } = require('./kaspa')
const fs = require('fs');
const path = require('path');

// Configuration  
const NETWORK_ID = 'devnet';
const RPC_URL = 'ws://127.0.0.1:17610';
const PRIVATE_KEY = 'b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f';

// ZK Proof configuration  
const ZK_VERIFIER_TAG = 0x20; // Groth16
const ZK_PROOF_FILE = './groth.proof.hex';
const IMAGE_ID_FILE = './groth.image.hex'; // 32-byte image ID file
const JOURNAL_FILE='./groth.journal.hex';
async function zktest() {
    const privateKey = new PrivateKey(PRIVATE_KEY);
    const keypair = privateKey.toKeypair();
    const sourceAddress = keypair.toAddress(NETWORK_ID);
    
    // Load ZK proof from file
    let ZK_PROOF_DATA;
    try {
        const proofHex = fs.readFileSync(ZK_PROOF_FILE, 'utf8').trim();
        const cleanHex = proofHex.startsWith('0x') ? proofHex.slice(2) : proofHex;
        ZK_PROOF_DATA = Buffer.from(cleanHex, 'hex');
        console.log(`Loaded ZK proof: ${ZK_PROOF_DATA.length} bytes`);
        console.log(`Proof hex: ${cleanHex.substring(0, 64)}...`);
    } catch (error) {
        console.error(`Failed to load ZK proof from ${ZK_PROOF_FILE}:`, error.message);
        return;
    }

    // Load Image ID from file
    let EXPECTED_IMAGE_ID;
    try {
        const imageIdHex = fs.readFileSync(IMAGE_ID_FILE, 'utf8').trim();
        const cleanHex = imageIdHex.startsWith('0x') ? imageIdHex.slice(2) : imageIdHex;
        EXPECTED_IMAGE_ID = Buffer.from(cleanHex, 'hex');
        
        if (EXPECTED_IMAGE_ID.length !== 32) {
            throw new Error(`Image ID must be 32 bytes, got ${EXPECTED_IMAGE_ID.length}`);
        }
        
        console.log(`Loaded Image ID: ${cleanHex}`);
    } catch (error) {
        console.error(`Failed to load Image ID from ${IMAGE_ID_FILE}:`, error.message);
        return;
    }

    // Load Image ID from file
    let JOURNAL;
    try {
        const journalHex = fs.readFileSync(JOURNAL_FILE, 'utf8').trim();
        const cleanHex = journalHex.startsWith('0x') ? journalHex.slice(2) : journalHex;
        JOURNAL = Buffer.from(cleanHex, 'hex');
        
        if (JOURNAL.length !== 32) {
            throw new Error(`Image ID must be 32 bytes, got ${JOURNAL.length}`);
        }
        
        console.log(`Loaded Image ID: ${cleanHex}`);
    } catch (error) {
        console.error(`Failed to load Image ID from ${IMAGE_ID_FILE}:`, error.message);
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

        // Create P2SH redeem script with embedded tag and image ID
        // When executed, stack will be: [proof_data] (from signature script)
        // This script pushes: image_id, tag, then calls OpZkPrecompile
        // OpZkPrecompile pops: tag first, then proof_data, then image_id
        const redeemScript = new ScriptBuilder()
            .addData(JOURNAL)
            .addData(EXPECTED_IMAGE_ID)              // Push image ID onto stack
            .addData(Buffer.from([ZK_VERIFIER_TAG])) // Push tag (0x20) onto stack
            .addOp(Opcodes.OpZkPrecompile)           // Execute ZK verification
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

        // Build signature script for P2SH spending
        // Only need to provide: proof_data + redeem_script
        // The redeem script already contains the tag and image ID
        const signatureScript = new ScriptBuilder()
            .addData(ZK_PROOF_DATA)                    // Push proof data
            .addData(Buffer.from(redeemScript, 'hex')) // Push redeem script (P2SH requirement)
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
                amount: commitAmount - 140000n
            }],
            0n,
            '',
            104 
        );

        // Set the signature script
        redeemTx.inputs[0].signatureScript = signatureScript;

        console.log('Redeem transaction created');
        console.log('Redeem script contains:');
        console.log('  - Image ID:', EXPECTED_IMAGE_ID.toString('hex'));
        console.log('  - Tag: 0x20 (Groth16)');
        console.log('  - OpZkPrecompile');
        console.log('Signature script provides only the proof data');

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