const { PrivateKey, RpcClient, ScriptBuilder, Opcodes,
  payToScriptHashScript, addressFromScriptPublicKey,
  createTransaction, signTransaction,
  Encoding } = require('../../../../nodejs/kaspa');
const fs = require('fs');
const path = require('path');

// Configuration
const NETWORK_ID = 'devnet';
const RPC_URL = 'ws://127.0.0.1:17610';
const PRIVATE_KEY = 'b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f';


// ZK Proof configuration
const ZK_VERIFIER_TAG = 0x21; // R0Succinct

// Data files matches the stack layout expected by R0SuccinctPrecompile::verify_zk
// Stack (bottom to top): seal, claim, hashfn, control_index, control_digests, journal, image_id, tag
const SEAL_FILE = './data/direct/succinct.seal.hex';
const CLAIM_FILE = './data/direct/succinct.claim.hex';
const HASHFN_FILE = './data/direct/succinct.hashfn.hex';
const CONTROL_INDEX_FILE = './data/direct/succinct.control_index.hex';
const CONTROL_DIGESTS_FILE = './data/direct/succinct.control_digests.hex';
const CONTROL_ID_FILE = './data/direct/succinct.control_id.hex';
const JOURNAL_FILE = './data/direct/succinct.journal.hex';
const IMAGE_ID_FILE = './data/direct/succinct.image.hex';

/**
 * @param {fs.PathOrFileDescriptor} filePath
 * @param {string} label
 */
function loadHexFile(filePath, label) {
  const hex = fs.readFileSync(filePath, 'utf8').trim();
  const cleanHex = hex.startsWith('0x') ? hex.slice(2) : hex;
  const buf = Buffer.from(cleanHex, 'hex');
  console.log(`Loaded ${label}: ${buf.length} bytes`);
  return buf;
}

async function succinctVerifyNoBuilder() {
  const privateKey = new PrivateKey(PRIVATE_KEY);
  const keypair = privateKey.toKeypair();
  const sourceAddress = keypair.toAddress(NETWORK_ID);
  console.log(`Using source address: ${sourceAddress}`);

  // Load all proof components
  let seal, claim, hashfn, controlIndex, controlDigests, journal, imageId, controlId;
  try {
    seal = loadHexFile(SEAL_FILE, 'seal');
    claim = loadHexFile(CLAIM_FILE, 'claim');
    hashfn = loadHexFile(HASHFN_FILE, 'hashfn');
    controlIndex = loadHexFile(CONTROL_INDEX_FILE, 'control_index');
    controlDigests = loadHexFile(CONTROL_DIGESTS_FILE, 'control_digests');
    controlId = loadHexFile(CONTROL_ID_FILE, 'control_id');
    journal = loadHexFile(JOURNAL_FILE, 'journal');
    imageId = loadHexFile(IMAGE_ID_FILE, 'image_id');
  } catch (error) {
    console.error('Failed to load proof data:', error);
    return;
  }

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

    // Create P2SH redeem script
    const redeemScript = new ScriptBuilder({
      flags: {
        covenantsEnabled:true
      }
    })
      .addData(imageId)
      .addData(controlId)
      .addData(hashfn)
      .addData(Buffer.from([ZK_VERIFIER_TAG]))
      .addOp(Opcodes.OpZkPrecompile)
      .drain();

    const lockingScript = payToScriptHashScript(redeemScript);
    console.log(`Locking script created:`, lockingScript);
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
    const commitTxId = submitResult.transactionId;
    console.log(`Commit transaction submitted: ${commitTxId}`);

    // Wait for commit transaction confirmation
    console.log('Waiting for commit transaction to be accepted...');
    await new Promise(resolve => setTimeout(resolve, 5000));

    // Create REDEEM transaction
    console.log('Creating redeem transaction...');

    // Build signature script for P2SH spending.
    // Push proof components that are NOT in the redeem script, then the redeem script itself.
    // These go onto the stack before the redeem script executes.
    const signatureScript = new ScriptBuilder({
      flags:{
        covenantsEnabled:true
      }
    })
      .addData(claim)
      .addData(controlIndex)
      .addData(controlDigests)
      .addData(seal)
      .addData(journal)
      .addData(Buffer.from(redeemScript, 'hex'))
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
        amount: commitAmount - 47363400n
      }],
      0n,
      '',
      0
    );

    console.log(redeemTx)

    // Set the signature script & compute budget
    redeemTx.inputs[0].signatureScript = signatureScript;
    redeemTx.inputs[0].computeBudget = 2500;
    redeemTx.version = 1;

    console.log('Redeem transaction created');
    console.log('Redeem script contains:');
    console.log('  - Journal:', journal.toString('hex'));
    console.log('  - Image ID:', imageId.toString('hex'));
    console.log('  - Tag: 0x21 (R0Succinct)');
    console.log('  - OpZkPrecompile');
    console.log('Signature script provides: seal, claim, hashfn, control_index, control_digests');

    // Submit redeem transaction
    const redeemResult = await rpc.submitTransaction({ transaction: redeemTx });
    const redeemTxId = redeemResult.transactionId;
    console.log(`Redeem transaction submitted: ${redeemTxId}`);
    console.log('ZK proof verification successful!');

  } catch (error) {
    console.error('Error:', error);
  } finally {
    await rpc.disconnect();
  }



}

succinctVerifyNoBuilder().catch(console.error);