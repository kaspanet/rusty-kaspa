// Groth16 zk-to-script: the LOW-LEVEL "split" flow.
//
// Unlike `groth16_builder.js` (which uses `ZkScriptBuilder`'s staged commit/finalize flow),
// this example composes the two halves of the locking/unlocking scripts with
// the low-level free-function bindings with the same builder's low-level fragment methods:
//
//   * `appendR0Groth16Verifier`      — the *verifier* fragment (redeem script).
//                                       Built by the covenant author.
//   * `pushR0Groth16Witness`         — the *witness* push (signature script).
//                                       Built by the spender / tx builder.
//
// These two sides are typically produced by different software at different
// times: the verifier locks the funds; the witness unlocks them once a proof
// exists. The journal hash is *caller-owned* — the SDK never pushes it for you.
//
// Run after building the wasm module (`./build-node` in wasm/). Requires a
// devnet node at the RPC url below. If `./kaspa` does not resolve, point the
// require at the build output (`../../../../nodejs/kaspa`).

const { PrivateKey, RpcClient,
    ZkScriptBuilder, payToScriptHashScript, addressFromScriptPublicKey,
    createTransaction, signTransaction,
    Encoding } = require('./kaspa');
const fs = require('fs');
const path = require('path');

// --- Configuration ---
const NETWORK_ID = 'devnet';
const RPC_URL = 'ws://127.0.0.1:17610';
const PRIVATE_KEY = 'b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f';

// --- ZK fixtures (same program/proof as groth16_builder.js) ---
const IMAGE_ID = '75641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0';
const JOURNAL_HASH = '5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456';
// borsh-encoded `Groth16Receipt<ReceiptClaim>`. Read from file to keep this
// example small (it is the same receipt hard-coded in groth16_builder.js).
const GROTH16_RECEIPT = fs.readFileSync(path.join(__dirname, 'builder_data', 'groth.rcpt.hex'), 'utf8').trim();

const FLAGS = { flags: { covenantsEnabled: true } };

// Covenant-author side — the verifier fragment (redeem script).

// Dynamic journal: the journal hash is supplied at spend time by the signature
// script. `appendR0Groth16Verifier` embeds the image id, the fixed r0 groth16
// verifier params / vk, the in-script receipt-claim reconstruction and the
// precompile call. At spend time it expects `[..., journal_hash, proof]`.
function buildRedeemScript() {
    const builder = ZkScriptBuilder.newR0(FLAGS);
    builder.appendR0Groth16Verifier(IMAGE_ID);
    return builder.drain();
}

// Fixed journal: bind the covenant to one specific journal hash, baked into the
// redeem script. The spender then only has to supply the proof. At spend time
// the verifier expects just `[..., proof]`.
function buildFixedJournalRedeemScript() {
    const builder = ZkScriptBuilder.newR0(FLAGS);
    builder.appendR0Groth16VerifierWithFixedJournal(IMAGE_ID, JOURNAL_HASH);
    return builder.drain();
}

// Spender / tx-builder side — the witness push (signature script).
// A P2SH signature script must be push-only; both helpers below are.

// Dynamic journal: the caller owns the journal hash, so it is pushed
// for groth16 it must sit *under* the proof. `pushR0Groth16Witness` then maps
// the receipt to the compressed proof and pushes it on top. Finally the redeem
// script itself is pushed so the P2SH engine can execute it.
function buildSignatureScript(redeemScript) {
    const builder = ZkScriptBuilder.newR0(FLAGS);
    builder.addData(JOURNAL_HASH);
    builder.pushR0Groth16Witness(GROTH16_RECEIPT);
    builder.addData(Buffer.from(redeemScript, 'hex'));
    return builder.drain();
}

// Fixed journal: the journal hash is already in the redeem script, so the
// signature script carries only the proof.
function buildFixedJournalSignatureScript(redeemScript) {
    const builder = ZkScriptBuilder.newR0(FLAGS);
    builder.pushR0Groth16Witness(GROTH16_RECEIPT);
    builder.addData(Buffer.from(redeemScript, 'hex'));
    return builder.drain();
}

async function groth16VerifySplit() {
    const privateKey = new PrivateKey(PRIVATE_KEY);
    const keypair = privateKey.toKeypair();
    const sourceAddress = keypair.toAddress(NETWORK_ID);

    // Build both halves up front. In a real deployment the redeem script is
    // built when locking the funds and the signature script much later, when a
    // proof exists, possibly by entirely separate software.
    const redeemScript = buildRedeemScript();
    const signatureScript = buildSignatureScript(redeemScript);
    const lockingScript = payToScriptHashScript(redeemScript);

    console.log('Groth16 split flow ---');
    console.log(`Redeem (verifier) script: ${redeemScript}`);
    console.log(`Signature (witness) script length: ${Buffer.from(signatureScript, 'hex').length} bytes`);

    // Fixed-journal covenant variant (built but not spent here) — shown for
    // reference. Note its redeem script differs (the journal is baked in).
    const fixedRedeem = buildFixedJournalRedeemScript();
    const fixedSig = buildFixedJournalSignatureScript(fixedRedeem);
    console.log(`Fixed-journal redeem script: ${fixedRedeem}`);
    console.log(`Fixed-journal signature length: ${Buffer.from(fixedSig, 'hex').length} bytes`);

    const p2shAddress = addressFromScriptPublicKey(lockingScript, NETWORK_ID);
    if (!p2shAddress) {
        console.error('Failed to derive P2SH address from redeem script');
        return;
    }
    console.log(`P2SH address: ${p2shAddress}`);

    const rpc = new RpcClient({ url: RPC_URL, encoding: Encoding.Borsh, networkId: NETWORK_ID });
    await rpc.connect();

    try {
        console.log('Fetching UTXOs...');
        const response = await rpc.getUtxosByAddresses([sourceAddress]);
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

        // Commit transaction: fund the P2SH address
        const utxoToSpend = matureUtxos[0];
        const commitAmount = utxoToSpend.amount - 163500n;

        const utxoEntries = [{
            address: sourceAddress,
            outpoint: { transactionId: utxoToSpend.outpoint.transactionId, index: utxoToSpend.outpoint.index },
            scriptPublicKey: utxoToSpend.entry.scriptPublicKey,
            amount: utxoToSpend.amount,
            isCoinbase: utxoToSpend.entry.isCoinbase,
            blockDaaScore: utxoToSpend.entry.blockDaaScore,
        }];

        const commitTx = createTransaction(utxoEntries, [{ address: p2shAddress, amount: commitAmount }], 0n, '', 1);
        const signedCommitTx = signTransaction(commitTx, [privateKey], false);
        const submitResult = await rpc.submitTransaction({ transaction: signedCommitTx });
        const commitTxId = submitResult.transactionId;
        console.log(`Commit transaction submitted: ${commitTxId}`);

        console.log('Waiting for commit transaction to be accepted...');
        await new Promise(resolve => setTimeout(resolve, 5000));

        // Redeem transaction: unlock with the split-built signature script
        const p2shUtxoEntry = {
            address: p2shAddress,
            outpoint: { transactionId: commitTxId, index: 0 },
            scriptPublicKey: lockingScript,
            amount: commitAmount,
            isCoinbase: false,
            blockDaaScore: currentDaaScore,
        };

        const redeemTx = createTransaction([p2shUtxoEntry], [{ address: sourceAddress, amount: commitAmount - 16153300n }], 0n, '', 0);
        redeemTx.inputs[0].signatureScript = signatureScript;
        redeemTx.inputs[0].computeBudget = 1600;
        redeemTx.version = 1;

        console.log('Submitting redeem transaction with Groth16 proof verification...');
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

groth16VerifySplit().catch(console.error);
