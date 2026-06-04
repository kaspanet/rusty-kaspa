const { PrivateKey, RpcClient, ScriptBuilder, Opcodes,
    payToScriptHashScript, addressFromScriptPublicKey,
    createTransaction, signTransaction } = require('./kaspa');

// Configuration  
const NETWORK_ID = 'devnet';
const RPC_URL = 'ws://127.0.0.1:17610';
const PRIVATE_KEY = 'b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f';

// ZK Proof configuration - Hardcoded values
const ZK_VERIFIER_TAG = 0x20; // Groth16

const UNPREPARED_VK_HEX = 'e2f26dbea299f5223b646cb1fb33eadb059d9407559d7441dfd902e3a79a4d2dabb73dc17fbc13021e2471e0c08bd67d8401f52b73d6d07483794cad4778180e0c06f33bbc4c79a9cadef253a68084d382f17788f885c9afd176f7cb2f036789edf692d95cbdde46ddda5ef7d422436779445c5e66006a42761e1f12efde0018c212f3aeb785e49712e7a9353349aaf1255dfb31b7bf60723a480d9293938e1933033e7fea1f40604eaacf699d4be9aacc577054a0db22d9129a1728ff85a01a1c3af829b62bf4914c0bcf2c81a4bd577190eff5f194ee9bac95faefd53cb0030600000000000000e43bdc655d0f9d730535554d9caa611ddd152c081a06a932a8e1d5dc259aac123f42a188f683d869873ccc4c119442e57b056e03e2fa92f2028c97bc20b9078747c30f85444697fdf436e348711c011115963f855197243e4b39e6cbe236ca8ba7f2042e11f9255afbb6c6e2c3accb88e401f2aac21c097c92b3fbdb99f98a9b0dcd6c075ada6ed0ddfece1d4a2d005f61a7d5df0b75c18a5b2374d64e495fab93d4c4b1200394d5253cce2f25a59b862ee8e4cd43686603faa09d5d0d3c1c8f';

const PROOF_HEX = '570253c0c483a1b16460118e63c155f3684e784ae7d97e8fc3f544128b37fe15075eab5ac31150c8a44253d8525971241bbd7227fcefbae2db4ae71675c56a2e0eb9235136b15ab72f16e707832f3d6ae5b0ba7cca53ae17cb52b3201919eb9d908c16297abd90aa7e00267bc21a9a78116e717d4d76edd44e21cca17e3d592d';

// Public inputs (5 field elements, 32 bytes each, little-endian)
const PUBLIC_INPUTS = [
    'a54dc85ac99f851c92d7c96d7318af4100000000000000000000000000000000',
    'dbe7c0194edfcc37eb4d422a998c1f5600000000000000000000000000000000',
    'a95ac0b37bfedcd8136e6c1143086bf500000000000000000000000000000000',
    'd223ffcb21c6ffcb7c8f60392ca49dde00000000000000000000000000000000',
    'c07a65145c3cb48b6101962ea607a4dd93c753bb26975cb47feb00d3666e4404'
];

async function groth16Verify() {
    const privateKey = new PrivateKey(PRIVATE_KEY);
    const keypair = privateKey.toKeypair();
    const sourceAddress = keypair.toAddress(NETWORK_ID);
    
    // Parse proof data
    const unpreparedVk = Buffer.from(UNPREPARED_VK_HEX, 'hex');
    const proof = Buffer.from(PROOF_HEX, 'hex');
    const publicInputs = PUBLIC_INPUTS.map(hex => Buffer.from(hex, 'hex'));
    const numInputs = PUBLIC_INPUTS.length;

    console.log(`Verifying Groth16 proof with:`);
    console.log(`  - Verifying key: ${unpreparedVk.length} bytes`);
    console.log(`  - Proof: ${proof.length} bytes`);
    console.log(`  - Public inputs: ${numInputs} field elements`);

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
        // Stack layout when executed:
        // 1. Signature script pushes: input4, input3, input2, input1, input0, num_inputs(5), proof, vk
        // 2. Redeem script pushes: tag (0x20)
        // 3. OpZkPrecompile pops in order: tag, vk, proof, num_inputs, input0...input4
        const redeemScriptBuilder = new ScriptBuilder();
        
        // The signature script will push all the proof data
        // The redeem script only needs to push the tag and call the opcode
        redeemScriptBuilder.addData(Buffer.from([ZK_VERIFIER_TAG])); // Push tag (0x20)
        redeemScriptBuilder.addOp(Opcodes.OpZkPrecompile);            // Execute ZK verification
        
        const redeemScript = redeemScriptBuilder.drain();
        const lockingScript = payToScriptHashScript(redeemScript);
        
        console.log(`Redeem script (hex): ${redeemScript}`);

        // Convert the P2SH script to an address
        const p2shAddress = addressFromScriptPublicKey(lockingScript, NETWORK_ID);
        console.log(`P2SH address: ${p2shAddress}`);

        // Create COMMIT transaction
        const utxoToSpend = matureUtxos[0];
        const commitAmount = utxoToSpend.amount - 10000n;

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

        const signedCommitTx = signTransaction(commitTx, [privateKey], false);
        console.log('Commit transaction signed');

        const submitResult = await rpc.submitTransaction({ transaction: signedCommitTx });
        const commitTxId = submitResult.transactionId || submitResult;
        console.log(`Commit transaction submitted: ${commitTxId}`);

        // Wait for commit transaction confirmation
        console.log('Waiting for commit transaction to be accepted...');
        await new Promise(resolve => setTimeout(resolve, 5000));

        // Create REDEEM transaction
        console.log('Creating redeem transaction...');

        // Build signature script with proof data
        // Stack order (bottom to top): vk, proof, num_inputs, input0, input1, input2, input3, input4
        const signatureScriptBuilder = new ScriptBuilder();
        
        
        
        
        // Push public inputs in order (input0 first, pushed last so it's on top)
        for (let i = publicInputs.length - 1; i >= 0; i--) {
            signatureScriptBuilder.addData(publicInputs[i]);
        }
        signatureScriptBuilder.addI64(BigInt(numInputs));

        // Push verifying key
        // Push proof
        signatureScriptBuilder.addData(proof);
        // Push number of inputs (little-endian u16)
        
                signatureScriptBuilder.addData(unpreparedVk);

        // Push redeem script (P2SH requirement)
        signatureScriptBuilder.addData(Buffer.from(redeemScript, 'hex'));
        
        const signatureScript = signatureScriptBuilder.drain();

        console.log(`Signature script length: ${Buffer.from(signatureScript, 'hex').length} bytes`);
        console.log('Stack contents (signature script):');
        console.log(`  - Verifying key: ${unpreparedVk.length} bytes`);
        console.log(`  - Proof: ${proof.length} bytes`);
        console.log(`  - Number of inputs: ${numInputs}`);
        publicInputs.forEach((input, idx) => {
            console.log(`  - Input ${idx}: ${input.toString('hex').substring(0, 32)}...`);
        });

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
                amount: commitAmount - 142000n
            }],
            0n,
            '',
            104
        );

        // Set the signature script
        redeemTx.inputs[0].signatureScript = signatureScript;

        console.log('Redeem transaction created');
        console.log('Submitting redeem transaction with Groth16 proof verification...');

        // Submit redeem transaction
        const redeemResult = await rpc.submitTransaction({ transaction: redeemTx });
        const redeemTxId = redeemResult.transactionId || redeemResult;
        console.log(`✓ Redeem transaction submitted: ${redeemTxId}`);
        console.log('✓ Groth16 proof verification successful!');

    } catch (error) {
        console.error('Error:', error);
        if (error.stack) {
            console.error('Stack:', error.stack);
        }
    } finally {
        await rpc.disconnect();
    }
}

groth16Verify().catch(console.error);