const {
    initConsolePanicHook,
    Transaction,
    TransactionInput,
    TransactionOutput,
    ScriptPublicKey,
    GenesisCovenantGroup,
    CovenantBinding,
    Hash,
    covenantId,
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

function expectError(label, fn, expectedSubstring) {
    try {
        fn();
        console.log(`  ${label}: FAIL (no error thrown)`);
    } catch (e) {
        const msg = e.message || String(e);
        const pass = msg.includes(expectedSubstring);
        console.log(`  ${label}:`, pass, pass ? '' : `(got: ${msg})`);
    }
}

(() => {
    const tx = makeTx(2, 4);

    console.log('--- Before populateGenesisCovenants ---');
    for (let i = 0; i < tx.outputs.length; i++) {
        console.log(`  output[${i}] covenant:`, tx.outputs[i].covenant);
    }

    // Example 1 — GenesisCovenantGroup instances
    // Group A binds outputs [0, 2] via authorizing input 0.
    // Group B binds outputs [1, 3] via authorizing input 1.
    console.log('\n--- Example 1: Populate using GenesisCovenantGroup instances ---');
    const groupA = new GenesisCovenantGroup(0, [0, 2]);
    const groupB = new GenesisCovenantGroup(1, [1, 3]);
    tx.populateGenesisCovenants([groupA, groupB]);

    for (let i = 0; i < tx.outputs.length; i++) {
        const cov = tx.outputs[i].covenant;
        console.log(`  output[${i}] covenant id: ${cov.covenantId}, authorizingInput: ${cov.authorizingInput}`);
    }

    // Outputs in the same group share the same covenant id.
    const covId0 = tx.outputs[0].covenant.covenantId;
    const covId2 = tx.outputs[2].covenant.covenantId;
    console.log('\nGroup A output covenant ids match:', covId0.toString() === covId2.toString());

    const covId1 = tx.outputs[1].covenant.covenantId;
    const covId3 = tx.outputs[3].covenant.covenantId;
    console.log('Group B output covenant ids match:', covId1.toString() === covId3.toString());

    // Different groups produce different covenant ids.
    console.log('Groups A and B covenant IDs differ:', covId0.toString() !== covId1.toString());

    // Example 2 — plain objects instead of GenesisCovenantGroup instances
    // Build a fresh transaction (covenants must not already be populated).
    console.log('\n--- Example 2: Populate using plain objects ---');
    const tx2 = makeTx(2, 4);

    tx2.populateGenesisCovenants([
        { authorizingInput: 0, outputs: [0, 2] },
        { authorizingInput: 1, outputs: [1, 3] },
    ]);

    for (let i = 0; i < tx2.outputs.length; i++) {
        const cov = tx2.outputs[i].covenant;
        console.log(`  output[${i}] covenant id: ${cov.covenantId}, authorizingInput: ${cov.authorizingInput}`);
    }

    // Both approaches must produce the same covenant ids.
    console.log('\n--- Consistency check ---');
    const allMatch =
        tx.outputs[0].covenant.covenantId.toString() === tx2.outputs[0].covenant.covenantId.toString() &&
        tx.outputs[1].covenant.covenantId.toString() === tx2.outputs[1].covenant.covenantId.toString();
    console.log('Covenant IDs from instance & plain objects match:', allMatch);

    // --- Success case: verify bound and unbound outputs ---
    // Mirrors the Rust test_populate_genesis_covenants success case.
    console.log('\n--- Success case: covenant bound & unbound outputs ---');
    const tx3 = makeTx(1, 8);
    const groupAOutputs = [1, 3, 7];
    const groupBOutputs = [2, 4, 5];

    // Pre-compute expected covenant ids.
    // covenantId() expects a plain outpoint object and an array of { index, output } objects.
    const outpoint = { transactionId, index: 0 };
    const expectedA = covenantId(outpoint, groupAOutputs.map(i => ({ index: i, output: tx3.outputs[i] })));
    const expectedB = covenantId(outpoint, groupBOutputs.map(i => ({ index: i, output: tx3.outputs[i] })));

    tx3.populateGenesisCovenants([
        new GenesisCovenantGroup(0, groupAOutputs),
        new GenesisCovenantGroup(0, groupBOutputs),
    ]);

    // Outputs in group A should all share expectedA.
    for (const i of groupAOutputs) {
        const cov = tx3.outputs[i].covenant;
        console.log(`  output[${i}] bound to group A:`,
            cov.covenantId.toString() === expectedA.toString() && cov.authorizingInput === 0);
    }
    // Outputs in group B should all share expectedB.
    for (const i of groupBOutputs) {
        const cov = tx3.outputs[i].covenant;
        console.log(`  output[${i}] bound to group B:`,
            cov.covenantId.toString() === expectedB.toString() && cov.authorizingInput === 0);
    }
    // Outputs 0 and 6 must remain unbound.
    console.log('  output[0] unbound:', tx3.outputs[0].covenant === undefined);
    console.log('  output[6] unbound:', tx3.outputs[6].covenant === undefined);

    // --- Error cases ---
    // Each case should throw; we verify the error message matches the expected Rust error.
    console.log('\n--- Error cases ---');

    // 1. Authorizing input index out of bounds.
    expectError('NoSuchInput', () => {
        const t = makeTx(1, 1);
        t.populateGenesisCovenants([new GenesisCovenantGroup(1, [0])]);
    }, 'out of bounds for 1 inputs');

    // 2. Output index out of bounds.
    expectError('NoSuchOutput', () => {
        const t = makeTx(1, 1);
        t.populateGenesisCovenants([new GenesisCovenantGroup(0, [1])]);
    }, 'out of bounds for 1 outputs');

    // 3. Empty outputs list.
    expectError('EmptyOutputs', () => {
        const t = makeTx(1, 1);
        t.populateGenesisCovenants([new GenesisCovenantGroup(0, [])]);
    }, 'outputs list is empty');

    // 4. Outputs not sorted.
    expectError('OutputsNotOrdered', () => {
        const t = makeTx(1, 4);
        t.populateGenesisCovenants([new GenesisCovenantGroup(0, [1, 3, 2])]);
    }, 'not strictly ordered');

    // 5. Overlapping outputs across groups.
    expectError('OutputsNotDisjoint', () => {
        const t = makeTx(1, 5);
        t.populateGenesisCovenants([
            new GenesisCovenantGroup(0, [1, 3]),
            new GenesisCovenantGroup(0, [2, 3]),
        ]);
    }, 'appears in more than one group');

    // 6. Output already has a covenant binding.
    expectError('CovenantAlreadyPopulated', () => {
        const dummyCovenant = new CovenantBinding(0, new Hash('0000000000000000000000000000000000000000000000000000000000000007'));
        const t = new Transaction({
            version: 1,
            inputs: [new TransactionInput({
                previousOutpoint: { transactionId, index: 0 },
                signatureScript: '', sequence: 0n, sigOpCount: 0,
            })],
            outputs: [
                new TransactionOutput(100n, spk),
                new TransactionOutput(100n, spk, dummyCovenant),
            ],
            lockTime: 0n, gas: 0n, payload: '',
            subnetworkId: '0000000000000000000000000000000000000000',
        });
        t.populateGenesisCovenants([new GenesisCovenantGroup(0, [1])]);
    }, 'already populated');
})();
