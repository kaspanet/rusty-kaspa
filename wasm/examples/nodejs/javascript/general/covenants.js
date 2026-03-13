const {
    initConsolePanicHook,
    covenantId,
    Transaction,
    TransactionInput,
    TransactionOutput,
    ScriptPublicKey,
    GenesisCovenantGroup,
} = require('../../../../nodejs/kaspa');

initConsolePanicHook();

// A random transaction id (hex).
const transactionId = '4d0f8fd4428f043de5dec28b21487286c8fde96414448811f90934c4146332eb';

// A 32-byte script from a random tx (no version prefix).
const SCRIPT_HEX = '204eb0e58854c5f382234563bba5f05b3d20b7577089032e6436bb85068676da3eac';

const spk = new ScriptPublicKey(0, Buffer.from(SCRIPT_HEX, 'hex'));

function makeTx(numInputs, numOutputs) {
    const inputs = Array.from({ length: numInputs }, (_, i) => new TransactionInput({
        previousOutpoint: { transactionId, index: i },
        signatureScript: '',
        sequence: BigInt(i),
        sigOpCount: 0,
    }));
    const outputs = Array.from({ length: numOutputs }, () => new TransactionOutput(100n, spk));
    return new Transaction({
        version: 1, inputs, outputs, lockTime: 0n, gas: 0n, payload: '',
        subnetworkId: '0000000000000000000000000000000000000000',
    });
}

(() => {
    // Example computation of covenant ID via covenantId function
    console.log('--- Example: Compute covenant ID ---');
    const genesisOutpoint = { transactionId, index: 0 };
    const authOutputs = [
        { index: 0, output: new TransactionOutput(1000n, spk) },
        { index: 1, output: new TransactionOutput(2000n, spk) },
    ];
    const id = covenantId(genesisOutpoint, authOutputs);
    console.log('covenant id:', id.toString());

    // Example populate genesis covenants on a transaction
    console.log('\n--- Example: Populate genesis covenants ---');
    const tx = makeTx(2, 4);

    tx.populateGenesisCovenants([
        new GenesisCovenantGroup(0, [0, 1]),
        new GenesisCovenantGroup(1, [2, 3]),
    ]);

    for (let i = 0; i < tx.outputs.length; i++) {
        const cov = tx.outputs[i].covenant;
        console.log(`  output[${i}] covenantId: ${cov.covenantId}, authorizingInput: ${cov.authorizingInput}`);
    }

    // Outputs in the same group share a covenant ID; different groups differ.
    const covA = tx.outputs[0].covenant.covenantId.toString();
    const covB = tx.outputs[2].covenant.covenantId.toString();
    console.log('\nGroup A ids match:', covA === tx.outputs[1].covenant.covenantId.toString());
    console.log('Group B ids match:', covB === tx.outputs[3].covenant.covenantId.toString());
    console.log('Groups A and B differ:', covA !== covB);
})();
