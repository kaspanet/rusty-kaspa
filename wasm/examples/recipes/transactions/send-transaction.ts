// Build, sign, and submit a transaction on testnet.
//
//   cd wasm/examples
//   KASPA_PRIVATE_KEY="<64 hex chars>" npx tsx recipes/transactions/send-transaction.ts --to=kaspatest:...
//
// If --to is omitted, funds are sent back to the source address.

import {
    PrivateKey,
    Resolver,
    RpcClient,
    kaspaToSompi,
    sompiToKaspaString,
    createTransactions,
    initConsolePanicHook,
} from 'kaspa';
import { getNetworkId } from '../../shared/network';
import { requirePrivateKeyHex } from '../../shared/secrets';

initConsolePanicHook();

(async () => {
    const networkId = getNetworkId();
    const privateKey = new PrivateKey(requirePrivateKeyHex());
    const sourceAddress = privateKey.toKeypair().toAddress(networkId).toString();
    console.log('source:', sourceAddress);

    const toFlag = process.argv.slice(2).find((a) => a.startsWith('--to='));
    const destination = (toFlag && toFlag.split('=')[1]) || sourceAddress;

    // Amounts are ALWAYS sompi (1 KAS = 100,000,000 sompi). Never use floats.
    const amount = kaspaToSompi('0.2');

    const rpc = new RpcClient({ resolver: new Resolver(), networkId });
    await rpc.connect();

    const { isSynced } = await rpc.getServerInfo();
    if (!isSynced) {
        console.error('the node is not synced yet — try again shortly');
        await rpc.disconnect();
        return;
    }

    const { entries } = await rpc.getUtxosByAddresses([sourceAddress]);
    if (!entries.length) {
        console.error('no UTXOs at source address — fund it on testnet first');
        await rpc.disconnect();
        return;
    }

    const { transactions, summary } = await createTransactions({
        entries,
        outputs: [{ address: destination, amount }],
        priorityFee: 0n,
        changeAddress: sourceAddress,
        networkId,
    });

    console.log(`sending ${sompiToKaspaString(amount)} to ${destination}`);
    console.log(`network fee: ${sompiToKaspaString(summary.fees)}`);

    for (const pending of transactions) {
        await pending.sign([privateKey]); // signs in memory; the key never leaves this process
        const txid = await pending.submit(rpc);
        console.log('submitted:', txid);
    }

    await rpc.disconnect();
})();
