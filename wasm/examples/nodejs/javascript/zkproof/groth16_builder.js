const { PrivateKey, RpcClient, Opcodes,
    payToScriptHashScript, addressFromScriptPublicKey,
    createTransaction, signTransaction,
    Encoding, ZkScriptBuilder } = require('./kaspa');

// Configuration
const NETWORK_ID = 'devnet';
const RPC_URL = 'ws://127.0.0.1:17610';
const PRIVATE_KEY = 'b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f';


// ZK Proof configuration - Hardcoded values
const GROTH16_SER_RCPT = '000100002000d744924a379a5c5ef221d7c4342c02d2c64adf39cff869a2a2b47aadf3a61ad5de3ecad7f19b522777f15079fad1036120d074ada560fa75a8b87dc7d5ca25a9cec46e55efd0111824abcf94d350c6356e953556d30c5c000da8d3e414871e5ab6609c1359b578c128d27bdfea4f1b266190823504026dbca88a9a81ddc02e64efebc7589408cd3cdb3772fab1de01549f0c8b445d9496e56eb37ee5183727cc03b453446b72f61bdb1df401a6eb309f33420cc567e61a2fedecb93a46bf1a8b782beed89d6d7a9bba7f4882432725fc2609c73ef15c751be2d8fe77f20f17835c4d6e8a86224863bb5e94e4940fa5a6491368a1273125aea7d3561a7b7f01ce0d07bf6d1fe5e8c210616c1c3aeebba17407483e1844dbab6b11791bc26ab5e71e6620a834f4d8dc1998476b865a9a6437a14474f9442cf8664f9ec22a5eee';

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


        const zkScriptBuilder = ZkScriptBuilder.newR0({ flags: { covenantsEnabled: true } });
        zkScriptBuilder.commitToGroth16("1202d1e01d53a986c4a3250bc48fb78e7ca1533bc52836b6d4c969b5d5f798b2")
        const redeemScript = zkScriptBuilder.script();
        const signatureScript = zkScriptBuilder.finalizeWithGroth16Proof(GROTH16_SER_RCPT, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
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