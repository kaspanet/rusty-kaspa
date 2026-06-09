const { PrivateKey, RpcClient, Opcodes,
    payToScriptHashScript, addressFromScriptPublicKey,
    createTransaction, signTransaction,
    Encoding, R0ScriptBuilder } = require('./kaspa');

// Configuration  
const NETWORK_ID = 'devnet';
const RPC_URL = 'ws://127.0.0.1:17610';
const PRIVATE_KEY = 'b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f';


// ZK Proof configuration - Hardcoded values
const GROTH16_SER_RCPT = '0001000015fe378b1244f5c38f7ed9e74a784e68f355c1638e116064b1a183c4c0530257140cf216b9899d3a3fd8718db35946e75a5e69a2b4884935cd98d35c624c6ae41deb191920b352cb17ae53ca7cbab0e56a3d2f8307e7162fb75ab1365123b90e2e6ac57516e74adbe2baeffc2772bd1b24715952d85342a4c85011c35aab5e0728f81402ffe3655b3d07fe0a3df01a9b959ed54d2dccd4a955b77aa2ad08a1d103a01eb634d8f7ccb2ab903e053a0e0960a5b22f2d70d17f98dcb1936e940c2b2d593d7ea1cc214ed4ed764d7d716e11789a1ac27b26007eaa90bd7a29168c90142e64de0dfd31ffc775f3a5a31f87ff42cf78de195f8b78c3ea43f8b9a2cce101a95ac0b37bfedcd8136e6c1143086bf5d223ffcb21c6ffcb7c8f60392ca49dde73c457ba541936f0d907daf0c7253a39a9c5c427c225ba7709e44702d3c6eedc';

async function groth16Verify() {
    const privateKey = new PrivateKey(PRIVATE_KEY);
    const keypair = privateKey.toKeypair();
    const sourceAddress = keypair.toAddress(NETWORK_ID);

    // Parse proof data

    console.log(`Verifying Groth16 proof with:`);

    const rpc = new RpcClient({
        url: RPC_URL,
        encoding: Encoding.Borsh,
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


        const r0ScriptBuilder = new R0ScriptBuilder({ flags: { covenantsEnabled: true } });
        r0ScriptBuilder.commitToGroth16("75641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0")
        const redeemScript = r0ScriptBuilder.script();
        const signatureScript = r0ScriptBuilder.finalizeWithGroth16Proof(GROTH16_SER_RCPT, "5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456");
        const lockingScript = payToScriptHashScript(redeemScript);

        console.log(`Redeem script (hex): ${redeemScript}`);

        // Convert the P2SH script to an address
        const p2shAddress = addressFromScriptPublicKey(lockingScript, NETWORK_ID);

        if (!p2shAddress) {
            console.error('Failed to derive P2SH address from redeem script');
            return;
        }

        console.log(`P2SH address: ${p2shAddress}`);

        // Create COMMIT transaction
        const utxoToSpend = matureUtxos[0];
        const commitAmount = utxoToSpend.amount - 163500n;

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
        const commitTxId = submitResult.transactionId;
        console.log(`Commit transaction submitted: ${commitTxId}`);

        // Wait for commit transaction confirmation
        console.log('Waiting for commit transaction to be accepted...');
        await new Promise(resolve => setTimeout(resolve, 5000));

        // Create REDEEM transaction
        console.log('Creating redeem transaction...');
        console.log(`Signature script length: ${Buffer.from(signatureScript.sigScript, 'hex').length} bytes`);

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
                amount: commitAmount - 16153300n
            }],
            0n,
            '',
            0
        );

        // Set the signature script & compute budget
        redeemTx.inputs[0].signatureScript = signatureScript.sigScript;
        redeemTx.inputs[0].computeBudget = 1600;
        redeemTx.version=1;

        console.log('Redeem transaction created');
        console.log('Submitting redeem transaction with Groth16 proof verification...');

        // Submit redeem transaction
        const redeemResult = await rpc.submitTransaction({ transaction: redeemTx });
        const redeemTxId = redeemResult.transactionId || redeemResult;
        console.log(`Redeem transaction submitted: ${redeemTxId}`);
        console.log('Groth16 proof verification successful!');

    } catch (error) {
        console.error('Error:', error);
    } finally {
        await rpc.disconnect();
    }
}

groth16Verify().catch(console.error);