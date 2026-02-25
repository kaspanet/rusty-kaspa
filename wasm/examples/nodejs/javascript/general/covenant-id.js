const {
    initConsolePanicHook,
    covenantId,
    Hash,
    TransactionOutpoint,
    TransactionOutput,
    ScriptPublicKey
} = require('../../../../nodejs/kaspa');

initConsolePanicHook();

(() => {
    // Shared test data
    // A random transaction id (hex).
    const transactionId = '4d0f8fd4428f043de5dec28b21487286c8fde96414448811f90934c4146332eb';

    // A 32-byte script from random tx (no version prefix).
    const SCRIPT_HEX = '204eb0e58854c5f382234563bba5f05b3d20b7577089032e6436bb85068676da3eac';

    // ScriptPublicKey accepts three representations:
    const spkInstance = new ScriptPublicKey(0, Buffer.from(SCRIPT_HEX, 'hex'));
    const spkObject = { version: 0, script: SCRIPT_HEX };
    const spkHex = '0000' + SCRIPT_HEX;

    const VALUE_0 = 1000n;
    const VALUE_1 = 2000n;

    // Example 1 — plain object outpoint + plain object outputs (hex SPK)
    console.log('--- Example 1: plain object outpoint + plain object outputs (hex SPK) ---');
    const genesisOutpoint1 = { transactionId: transactionId, index: 0 };
    const authOutputs1 = [
        { index: 0, output: { value: VALUE_0, scriptPublicKey: spkHex } },
        { index: 1, output: { value: VALUE_1, scriptPublicKey: spkHex } },
    ];
    const id1 = covenantId(genesisOutpoint1, authOutputs1);
    console.log('covenant id:', id1.toString());

    // Example 2 — TransactionOutpoint instance + TransactionOutput instances
    console.log('\n--- Example 2: TransactionOutpoint instance + TransactionOutput instances ---');
    const genesisOutpoint2 = new TransactionOutpoint(new Hash(transactionId), 0);
    const authOutputs2 = [
        { index: 0, output: new TransactionOutput(VALUE_0, spkInstance) },
        { index: 1, output: new TransactionOutput(VALUE_1, spkInstance) },
    ];
    const id2 = covenantId(genesisOutpoint2, authOutputs2);
    console.log('covenant id:', id2.toString());

    // Example 3 — plain object outpoint + plain object outputs (object SPK)
    console.log('\n--- Example 3: plain object outpoint + ITransactionOutput with object SPK ---');
    const genesisOutpoint3 = { transactionId: transactionId, index: 0 };
    const authOutputs3 = [
        { index: 0, output: { value: VALUE_0, scriptPublicKey: spkObject } },
        { index: 1, output: { value: VALUE_1, scriptPublicKey: spkObject } },
    ]
    const id3 = covenantId(genesisOutpoint3, authOutputs3);
    console.log('covenant id:', id3.toString());

    // Example 4 — TransactionOutpoint instance + plain object outputs (hex SPK)
    console.log('\n--- Example 4: TransactionOutpoint instance + plain object outputs ---');
    const genesisOutpoint4 = new TransactionOutpoint(new Hash(transactionId), 0);
    const authOutputs4 = [
        { index: 0, output: { value: VALUE_0, scriptPublicKey: spkHex } },
        { index: 1, output: { value: VALUE_1, scriptPublicKey: spkHex } },
    ];
    const id4 = covenantId(genesisOutpoint4, authOutputs4);
    console.log('covenant id:', id4.toString());

    // All four calls use identical data — they must produce the same id.
    console.log('\n--- Consistency check ---');
    const allMatch =
        id1.toString() === id2.toString() &&
        id2.toString() === id3.toString() &&
        id3.toString() === id4.toString();
    console.log('All ids match:', allMatch);
})();