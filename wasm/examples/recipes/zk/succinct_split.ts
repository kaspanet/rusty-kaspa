// RISC0 succinct (STARK) zk-to-script: the LOW-LEVEL "split" flow.
//
//   cd wasm/examples
//   npx tsx recipes/zk/succinct_split.ts
//
// Unlike `succinct_builder.ts` (which uses `ZkScriptBuilder`'s staged commit/finalize flow),
// this example composes the two halves with the low-level free-function
// bindings with the same builder's low-level fragment methods:
//
//   * `appendR0SuccinctVerifier` — the *verifier* fragment (redeem script).
//                                  Built by the covenant author.
//   * `pushR0SuccinctWitness`    — the *witness* push (signature script).
//                                  Built by the spender / tx builder.
//
// The two sides are typically produced by different software at different
// times. The journal is *caller-owned* — the SDK never pushes it for you.
//
// Run after building the wasm module (`./build-node` in wasm/). Requires a
// devnet node at the RPC url below. If `./kaspa` does not resolve, point the
// require at the build output (`../../../../nodejs/kaspa`).

import { PrivateKey, RpcClient,
    ZkScriptBuilder, payToScriptHashScript, addressFromScriptPublicKey,
    createTransaction, signTransaction,
    Encoding } from 'kaspa';
import fs from 'fs';
import path from 'path';

const NETWORK_ID = 'devnet';
const RPC_URL = 'ws://127.0.0.1:17610';
const PRIVATE_KEY = 'b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f';

const IMAGE_ID = '1ade4c062dee368276ef6610bd7de59d9b63c7ebe87d8d75a63c0e288895cb7d';
const CONTROL_ID = '1ca3ca03030719064ba61b3125bdd326fc57f74e799ef860bdea6f3227381e16';
const JOURNAL = '5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456';
// `hashFnId` is omitted below, which defaults to "poseidon2" — currently the
// only supported hash function.

// borsh-encoded `SuccinctReceipt<ReceiptClaim>`. Read from file because the
// succinct receipt is large (~220 KB).
const SUCCINCT_RECEIPT = fs.readFileSync(path.join(__dirname, 'fixtures', 'receipts', 'succinct.rcpt.hex'), 'utf8').trim();

const FLAGS = { flags: { covenantsEnabled: true } };

// Covenant-author side — the verifier fragment (redeem script).

// Dynamic journal: supplied at spend time by the signature script.
// `appendR0SuccinctVerifier` embeds the image id, control id, hash function id
// and the precompile call. At spend time it expects
// `[..., claim, control_index, control_digests, seal, journal]`.
function buildRedeemScript() {
    const builder = ZkScriptBuilder.newR0(FLAGS);
    builder.appendR0SuccinctVerifier(IMAGE_ID, CONTROL_ID);
    return builder.drain();
}

// Fixed journal: bind the covenant to one specific journal, baked into the
// redeem script. At spend time the verifier expects just
// `[..., claim, control_index, control_digests, seal]`.
function buildFixedJournalRedeemScript() {
    const builder = ZkScriptBuilder.newR0(FLAGS);
    builder.appendR0SuccinctVerifierWithFixedJournal(IMAGE_ID, CONTROL_ID, undefined, JOURNAL);
    return builder.drain();
}

// Spender / tx-builder side, the witness push (signature script).
// A P2SH signature script must be push-only; both helpers below are.

// Dynamic journal: `pushR0SuccinctWitness` pushes the four receipt-derived
// items (claim, control_index, control_digests, seal). The caller owns the
// journal, so — for succinct it sits *on top* of the witness — it is pushed
// AFTER. Finally the redeem script itself is pushed for the P2SH engine.
function buildSignatureScript(redeemScript) {
    const builder = ZkScriptBuilder.newR0(FLAGS);
    builder.pushR0SuccinctWitness(SUCCINCT_RECEIPT);
    builder.addData(JOURNAL);
    builder.addData(Buffer.from(redeemScript, 'hex'));
    return builder.drain();
}

// Fixed journal: the journal is already in the redeem script, so the signature
// script carries only the four receipt-derived witness items.
function buildFixedJournalSignatureScript(redeemScript) {
    const builder = ZkScriptBuilder.newR0(FLAGS);
    builder.pushR0SuccinctWitness(SUCCINCT_RECEIPT);
    builder.addData(Buffer.from(redeemScript, 'hex'));
    return builder.drain();
}

async function succinctVerifySplit() {
    const privateKey = new PrivateKey(PRIVATE_KEY);
    const keypair = privateKey.toKeypair();
    const sourceAddress = keypair.toAddress(NETWORK_ID);

    // Build both halves up front. In a real deployment the redeem script is
    // built when locking the funds and the signature script much later, when a
    // proof exists — possibly by entirely separate software.
    const redeemScript = buildRedeemScript();
    const signatureScript = buildSignatureScript(redeemScript);
    const lockingScript = payToScriptHashScript(redeemScript);

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

        const redeemTx = createTransaction([p2shUtxoEntry], [{ address: sourceAddress, amount: commitAmount - 47363400n }], 0n, '', 0);
        redeemTx.inputs[0].signatureScript = signatureScript;
        redeemTx.inputs[0].computeBudget = 2500;
        redeemTx.version = 1;

        console.log('Submitting redeem transaction with succinct proof verification...');
        const redeemResult = await rpc.submitTransaction({ transaction: redeemTx });
        const redeemTxId = redeemResult.transactionId || redeemResult;
        console.log(`Redeem transaction submitted: ${redeemTxId}`);
        console.log('ZK proof verification successful!');
    } catch (error) {
        console.error('Error:', error);
    } finally {
        await rpc.disconnect();
    }
}

succinctVerifySplit().catch(console.error);
